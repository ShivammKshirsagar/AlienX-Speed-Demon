from pathlib import Path

root = Path(__file__).resolve().parents[2]
source = root / "track-a" / "src" / "main.rs"
target = Path(__file__).resolve().parent / "final-profile" / "src" / "main.rs"
text = source.read_text(encoding="utf-8")

def once(old: str, new: str) -> None:
    global text
    if text.count(old) != 1:
        raise SystemExit(f"expected one profiler insertion point: {old!r}")
    text = text.replace(old, new, 1)

once("fn main() {", "fn main() {\n    let total_clock = std::time::Instant::now();")
once("    bounds.push(data.len());\n\n    let mut locals =", "    bounds.push(data.len());\n\n    let parse_clock = std::time::Instant::now();\n    let mut locals =")
once("    let mut global_ids:", '    eprintln!("profile parse_ns={}", parse_clock.elapsed().as_nanos());\n    let merge_clock = std::time::Instant::now();\n    let mut global_ids:')
once("    // Independent groups sort in parallel; sorting all values gives the three exact ranks.", '    eprintln!("profile merge_ns={}", merge_clock.elapsed().as_nanos());\n    let selection_clock = std::time::Instant::now();\n    // Independent groups sort in parallel; sorting all values gives the three exact ranks.')
once("    let mut order: Vec<usize> =", '    eprintln!("profile selection_ns={}", selection_clock.elapsed().as_nanos());\n    let format_clock = std::time::Instant::now();\n    let mut order: Vec<usize> =')
once("    if let Some(parent) = Path::new(&output).parent() {", '    eprintln!("profile format_ns={}", format_clock.elapsed().as_nanos());\n    let write_clock = std::time::Instant::now();\n    if let Some(parent) = Path::new(&output).parent() {')
once("    drop(mapping);", '    eprintln!("profile write_ns={} total_ns={}", write_clock.elapsed().as_nanos(), total_clock.elapsed().as_nanos());\n    drop(mapping);')
target.write_text(text, encoding="utf-8", newline="\n")
