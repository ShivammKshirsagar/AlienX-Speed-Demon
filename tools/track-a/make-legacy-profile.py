from pathlib import Path
import shutil

here = Path(__file__).resolve().parent
source = here / "steps" / "packed-old"
target = here / "legacy-profile"
(target / "src").mkdir(parents=True, exist_ok=True)
shutil.copy2(source / "Cargo.toml", target / "Cargo.toml")
text = (source / "src" / "main.rs").read_text()
text = text.replace("fn main() {", "fn main() {\n    let main_clock = std::time::Instant::now();", 1)
text = text.replace(
    "    let mut locals = thread::scope(|scope| {",
    "    let parse_clock = std::time::Instant::now();\n    let mut locals = thread::scope(|scope| {",
    1,
)
text = text.replace(
    "    let mut endpoints: Vec<&[u8]> = locals",
    '    eprintln!("profile parse_ns={}", parse_clock.elapsed().as_nanos());\n'
    "    let merge_clock = std::time::Instant::now();\n"
    "    let mut endpoints: Vec<&[u8]> = locals",
    1,
)
text = text.replace(
    "    drop(locals);",
    '    eprintln!("profile merge_scatter_ns={}", merge_clock.elapsed().as_nanos());\n'
    "    drop(locals);",
    1,
)
text = text.replace(
    "    let total_work = record_count;",
    "    let selection_clock = std::time::Instant::now();\n    let total_work = record_count;",
    1,
)
text = text.replace(
    "    let hours = hour_strings();",
    '    eprintln!("profile selection_ns={}", selection_clock.elapsed().as_nanos());\n'
    "    let format_clock = std::time::Instant::now();\n"
    "    let hours = hour_strings();",
    1,
)
text = text.replace(
    "    if let Some(parent) = Path::new(&output).parent() {",
    '    eprintln!("profile format_ns={}", format_clock.elapsed().as_nanos());\n'
    "    let write_clock = std::time::Instant::now();\n"
    "    if let Some(parent) = Path::new(&output).parent() {",
    1,
)
text = text.replace(
    "    drop(mapping);\n    std::process::exit(0);",
    '    eprintln!("profile write_ns={} main_to_exit_ns={}", write_clock.elapsed().as_nanos(), main_clock.elapsed().as_nanos());\n'
    "    drop(mapping);\n    std::process::exit(0);",
    1,
)
(target / "src" / "main.rs").write_text(text)
