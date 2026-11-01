from __future__ import annotations

import hashlib
import importlib.util
import os
from pathlib import Path
import shutil
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[3]
HARNESS = Path(r"C:\GitHub Desktop\speed-demon-2026")
WORK = ROOT / ".out" / "track-a-variant-check"
SEED1_HASH = "495ca09682b9673ef836a506496177680bb4a3af54255174befddb1f83481cd3"


def run(command: list[str]) -> str:
    result = subprocess.run(command, capture_output=True, text=True)
    if result.returncode:
        raise RuntimeError(f"failed: {command}\n{result.stdout}\n{result.stderr}")
    return result.stdout


def make_data(name: str, rows: list[str]) -> Path:
    folder = WORK / "data" / name
    folder.mkdir(parents=True, exist_ok=True)
    (folder / "requests.log").write_text("".join(rows), encoding="ascii", newline="")
    return folder


def reference(folder: Path) -> bytes:
    out = WORK / "reference" / folder.name
    summary = out / "summary.txt"
    if summary.exists():
        return summary.read_bytes()
    if out.exists():
        shutil.rmtree(out)
    run([sys.executable, str(HARNESS / "reference" / "a_ingest.py"), "--data", str(folder), "--out", str(out)])
    return summary.read_bytes()


def invoke(image: str, folder: Path, label: str, memory: bool = False) -> tuple[bytes, int | None]:
    out = WORK / "out" / image / label
    if out.exists():
        shutil.rmtree(out)
    out.mkdir(parents=True)
    os.chmod(out, 0o777)
    args = ["docker", "run", "--rm", "--network=none", "--cpus=12", "--user", "0:0",
            "-v", f"{folder.resolve()}:/data:ro", "-v", f"{out.resolve()}:/out:rw"]
    peak = None
    if memory:
        shell = ('/app/solution /data/requests.log /out/summary.txt & p=$!; max=0; '
                 'while kill -0 "$p" 2>/dev/null; do '
                 'h=$(while read key value rest; do if [ "$key" = "VmHWM:" ]; then echo "$value"; break; fi; done < "/proc/$p/status"); '
                 'if [ -n "$h" ] && [ "$h" -gt "$max" ]; then max="$h"; fi; sleep 0.003; done; '
                 'wait "$p"; echo MAX_RSS_KB=$max')
        log = run(args + ["--entrypoint", "/bin/sh", f"sd-track-a-{image}", "-c", shell])
        marker = next((line for line in log.splitlines() if line.startswith("MAX_RSS_KB=")), None)
        if marker is None:
            raise RuntimeError(f"missing VmHWM for {image}/{label}: {log}")
        peak = int(marker.split("=", 1)[1])
    else:
        run(args + [f"sd-track-a-{image}", "/data/requests.log", "/out/summary.txt"])
    return (out / "summary.txt").read_bytes(), peak


def main() -> None:
    WORK.mkdir(parents=True, exist_ok=True)
    spec = importlib.util.spec_from_file_location("verify_a", ROOT / "tools" / "track-a" / "verify.py")
    verify = importlib.util.module_from_spec(spec)
    assert spec and spec.loader
    spec.loader.exec_module(verify)
    tests: list[tuple[str, Path, bytes]] = []
    worked = WORK / "data" / "worked"
    worked.mkdir(parents=True, exist_ok=True)
    (worked / "requests.log").write_text(verify.EXAMPLE, encoding="ascii", newline="")
    tests.append(("worked", worked, verify.EXPECTED.encode("ascii")))
    tests.append(("empty", make_data("empty", []), b""))
    single = make_data("single", ["2026-10-30T18:00:00.000001Z GET /only 200 9\n"])
    tests.append(("single", single, reference(single)))
    chunk = make_data("chunk-boundaries", [
        f"2026-10-30T{18 + i % 6:02d}:00:{i % 60:02d}.000001Z GET /boundary/{i:04d}{'x' * (i % 37)} {503 if i % 9 == 0 else 200} {i % 100000}\n"
        for i in range(240)
    ])
    tests.append(("chunk-boundaries", chunk, reference(chunk)))
    equal = make_data("equal-timestamps", [
        f"2026-10-30T18:00:00.123456Z GET /same 200 {i}\n" for i in range(200)
    ])
    tests.append(("equal-timestamps", equal, reference(equal)))
    hours = make_data("hour-boundaries", [
        "2026-10-30T18:59:59.999999Z GET /edge 200 1\n",
        "2026-10-30T19:00:00.000000Z GET /edge 500 2\n",
        "2026-10-31T23:59:59.999999Z POST /edge 503 3\n",
        "2026-11-01T00:00:00.000000Z POST /edge 200 4\n",
        "2026-11-02T17:59:59.999999Z GET /edge 200 5\n",
    ])
    tests.append(("hour-boundaries", hours, reference(hours)))
    distinct = make_data("distinct-endpoints", [
        f"2026-10-30T18:00:00.000000Z GET /distinct/{i:04d} 200 {i % 99999}\n"
        for i in range(5000)
    ])
    tests.append(("distinct-endpoints", distinct, reference(distinct)))
    for seed in (2, 3, 7):
        folder = ROOT / "tools" / "track-a" / "data" / f"seed{seed}"
        tests.append((f"seed{seed}-sample", folder, reference(folder)))
    seed2 = WORK / "data" / "seed2-full"
    if not (seed2 / "requests.log").exists():
        run([sys.executable, str(HARNESS / "generators" / "a_ingest.py"), "--seed", "2", "--scale", "full", "--out", str(seed2)])
    tests.append(("seed2-full", seed2, reference(seed2)))
    tests.append(("seed1-full", ROOT / "data" / "a", b""))

    first = int(os.environ.get("VARIANT_START", "1"))
    last = int(os.environ.get("VARIANT_END", "5"))
    for n in range(first, last + 1):
        image = f"v{n}"
        for label, folder, expected in tests:
            actual, peak = invoke(image, folder, label, label == "seed1-full")
            if label == "seed1-full":
                digest = hashlib.sha256(actual).hexdigest()
                if digest != SEED1_HASH:
                    raise AssertionError(f"{image} seed1 hash mismatch: {digest}")
                mapped_kb = (folder / "requests.log").stat().st_size // 1024
                print(f"{image} {label}: PASS {digest}; max RSS {peak} KB; proxy {max(0, (peak or 0) - mapped_kb)} KB")
            elif actual != expected:
                raise AssertionError(f"{image} {label}: output/reference mismatch")
            else:
                print(f"{image} {label}: PASS")


if __name__ == "__main__":
    main()
