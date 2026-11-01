from pathlib import Path
import shutil

ROOT = Path(__file__).resolve().parents[2]
BASE = Path(__file__).resolve().parent / "steps" / "packed-old"
TABLE = Path(__file__).resolve().parent / "steps" / "step-e"
OUT = Path(__file__).resolve().parent / "phase-probes"

for name, source in (("hash", BASE), ("table", TABLE)):
    target = OUT / name
    (target / "src").mkdir(parents=True, exist_ok=True)
    shutil.copy2(source / "Cargo.toml", target / "Cargo.toml")
    text = (source / "src" / "main.rs").read_text()
    text = text.replace(
        "    let mut locals = thread::scope(|scope| {",
        "    let parse_clock = std::time::Instant::now();\n"
        "    let mut locals = thread::scope(|scope| {",
        1,
    )
    text = text.replace(
        "    let mut endpoints: Vec<&[u8]> = locals",
        '    eprintln!("phase parse_ns={}", parse_clock.elapsed().as_nanos());\n'
        "    let mut endpoints: Vec<&[u8]> = locals",
        1,
    )
    (target / "src" / "main.rs").write_text(text)
