from pathlib import Path
import shutil

ROOT = Path(__file__).resolve().parents[2]
HERE = Path(__file__).resolve().parent
OUT = HERE / "selection-probes"
for name, source in (("sort", HERE / "steps" / "step-h"), ("select", ROOT / "track-a")):
    target = OUT / name
    (target / "src").mkdir(parents=True, exist_ok=True)
    shutil.copy2(source / "Cargo.toml", target / "Cargo.toml")
    text = (source / "src" / "main.rs").read_text()
    text = text.replace(
        "    thread::scope(|scope| {\n        let mut remaining_groups = groups.as_mut_slice();",
        "    let selection_clock = std::time::Instant::now();\n"
        "    thread::scope(|scope| {\n        let mut remaining_groups = groups.as_mut_slice();",
        1,
    )
    text = text.replace(
        "    let hours = hour_strings();",
        '    eprintln!("phase selection_ns={}", selection_clock.elapsed().as_nanos());\n'
        "    let hours = hour_strings();",
        1,
    )
    (target / "src" / "main.rs").write_text(text)
