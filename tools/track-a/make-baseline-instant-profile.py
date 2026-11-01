from pathlib import Path
import shutil

here = Path(__file__).resolve().parent
source = here / "baseline-snapshot2"
target = here / "baseline-instant-profile"
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
    "    let mut global_ids: FastMap<&[u8], usize> = FastMap::default();",
    '    eprintln!("profile parse_ns={}", parse_clock.elapsed().as_nanos());\n'
    "    let merge_clock = std::time::Instant::now();\n"
    "    let mut global_ids: FastMap<&[u8], usize> = FastMap::default();",
    1,
)
selection_mark = "    // Independent groups sort in parallel; sorting all values gives the three exact ranks."
text = text.replace(
    selection_mark,
    '    eprintln!("profile merge_ns={}", merge_clock.elapsed().as_nanos());\n'
    "    let selection_clock = std::time::Instant::now();\n"
    + selection_mark,
    1,
)
text = text.replace(
    "    let mut order: Vec<usize> = (0..endpoints.len()).collect();",
    '    eprintln!("profile selection_ns={}", selection_clock.elapsed().as_nanos());\n'
    "    let format_clock = std::time::Instant::now();\n"
    "    let mut order: Vec<usize> = (0..endpoints.len()).collect();",
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
    '    eprintln!("profile write_ns={} main_to_drop_ns={}", write_clock.elapsed().as_nanos(), main_clock.elapsed().as_nanos());\n'
    "    let exit_clock = std::time::Instant::now();\n"
    "    drop(mapping);\n"
    '    eprintln!("profile munmap_ns={}", exit_clock.elapsed().as_nanos());\n'
    "    std::process::exit(0);",
    1,
)
(target / "src" / "main.rs").write_text(text)
