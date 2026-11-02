"""Create tiny Track A edge fixtures and their Python-reference outputs."""
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parent / "out" / "final-gates"
REFERENCE = Path(r"C:\GitHub Desktop\speed-demon-2026\reference\a_ingest.py")
ROOT.mkdir(parents=True, exist_ok=True)


def write_case(name: str, lines: list[bytes]) -> None:
    folder = ROOT / name
    folder.mkdir(parents=True, exist_ok=True)
    (folder / "requests.log").write_bytes(b"".join(lines))
    subprocess.run(
        ["python", str(REFERENCE), "--data", str(folder), "--out", str(ROOT / f"{name}-ref")],
        check=True,
    )


def row(ts: str, method: str, endpoint: str, status: int, latency: int) -> bytes:
    return f"{ts} {method} {endpoint} {status} {latency}\n".encode("ascii")


edge = [
    row("2026-10-30T18:00:00.000000Z", "GET", "/", 200, 0),
    row("2026-10-30T18:59:59.999999Z", "POST", "/v2/orders", 499, 4294967295),
    row("2026-10-30T19:00:00.000000Z", "PUT", "/v2/orders-export", 500, 1),
    row("2026-10-31T23:59:59.999999Z", "PATCH", "/v2/orders.csv", 599, 10),
    row("2026-11-01T00:00:00.000000Z", "DELETE", "/v2/orders/{id}", 499, 100),
    row("2026-11-01T23:59:59.999999Z", "HEAD", "/v2/orders/{id}/items", 500, 1000),
    row("2026-11-02T00:00:00.000000Z", "OPTIONS", "/health", 599, 10000),
    row("2026-11-02T17:59:59.999999Z", "GET", "/internal/v1/tenants/{tenant_id}/projects/{project_id}/pipelines/{pipeline_id}/runs/{run_id}/artifacts/{artifact_id}:download", 200, 4294967295),
    row("2026-11-01T12:00:00.000000Z", "POST", "/same", 200, 7),
    row("2026-11-01T12:00:00.000000Z", "POST", "/same", 500, 7),
    row("2026-11-01T12:00:00.000000Z", "POST", "/same", 599, 7),
]
write_case("edge-all", edge)

exact = row("2026-10-30T18:00:00.000000Z", "GET", "/" + "a" * 24, 200, 0)
assert len(exact) == 64
write_case("exact-64", [exact])
write_case("exact-4096", [exact] * 64)
print(f"fixtures ready under {ROOT}")
