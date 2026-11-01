from pathlib import Path
import shutil

root = Path(__file__).resolve().parents[2]
source = root / "track-a"
target = Path(__file__).resolve().parent / "final-profile"
(target / "src").mkdir(parents=True, exist_ok=True)
shutil.copy2(source / "Cargo.toml", target / "Cargo.toml")
text = (source / "src" / "main.rs").read_text()
text = text.replace("fn main() {", "fn main() {\n    let main_clock = std::time::Instant::now();", 1)
text = text.replace(
    "        let data_ptr = data.as_ptr() as usize;",
    "        let parse_clock = std::time::Instant::now();\n        let data_ptr = data.as_ptr() as usize;",
    1,
)
text = text.replace(
    "        let worker_slots = Arc::clone(&local_slots);",
    "        let worker_slots = Arc::clone(&local_slots);\n"
    "        let worker_finishes = Arc::new((0..workers).map(|_| std::sync::atomic::AtomicU64::new(0)).collect::<Vec<_>>());\n"
    "        let finish_slots = Arc::clone(&worker_finishes);\n"
    "        let parse_origin = parse_clock;",
    1,
)
text = text.replace(
    "            let local = parse_range(data, bounds[id], bounds[id + 1]);",
    "            let local = parse_range(data, bounds[id], bounds[id + 1]);\n"
    "            finish_slots[id].store(parse_origin.elapsed().as_nanos() as u64, std::sync::atomic::Ordering::Relaxed);",
    1,
)
text = text.replace(
    "        let mut endpoints: Vec<&[u8]> = locals",
    '        eprintln!("profile parse_ns={} worker_finish_ns={:?}", parse_clock.elapsed().as_nanos(), worker_finishes.iter().map(|x| x.load(std::sync::atomic::Ordering::Relaxed)).collect::<Vec<_>>());\n'
    "        let merge_clock = std::time::Instant::now();\n"
    "        let mut endpoints: Vec<&[u8]> = locals",
    1,
)
text = text.replace(
    "        drop(locals);",
    '        eprintln!("profile merge_scatter_ns={}", merge_clock.elapsed().as_nanos());\n'
    "        drop(locals);",
    1,
)
text = text.replace(
    "        let group_ptr = groups.as_mut_ptr() as usize;",
    "        let selection_clock = std::time::Instant::now();\n"
    "        let group_ptr = groups.as_mut_ptr() as usize;",
    1,
)
text = text.replace(
    "        let hours = hour_strings();",
    '        eprintln!("profile selection_ns={}", selection_clock.elapsed().as_nanos());\n'
    "        let format_clock = std::time::Instant::now();\n"
    "        let hours = hour_strings();",
    1,
)
text = text.replace(
    "        let output_buffers: Vec<Vec<u8>> = Arc::try_unwrap(output_slots)",
    '        eprintln!("profile format_ns={}", format_clock.elapsed().as_nanos());\n'
    "        let output_buffers: Vec<Vec<u8>> = Arc::try_unwrap(output_slots)",
    1,
)
text = text.replace(
    "        if let Some(parent) = Path::new(&output).parent() {",
    "        let write_clock = std::time::Instant::now();\n"
    "        if let Some(parent) = Path::new(&output).parent() {",
    1,
)
text = text.replace(
    "                pool.stop();\n                // SAFETY: all output writes have completed before this process exit.",
    '                pool.stop();\n'
    '                eprintln!("profile write_ns={} main_to_exit_ns={}", write_clock.elapsed().as_nanos(), main_clock.elapsed().as_nanos());\n'
    "                // SAFETY: all output writes have completed before this process exit.",
    1,
)
text = text.replace(
    "        pool.stop();\n        // SAFETY: all positioned writes have joined; process teardown closes remaining handles.",
    '        pool.stop();\n'
    '        eprintln!("profile write_ns={} main_to_exit_ns={}", write_clock.elapsed().as_nanos(), main_clock.elapsed().as_nanos());\n'
    "        // SAFETY: all positioned writes have joined; process teardown closes remaining handles.",
    1,
)
(target / "src" / "main.rs").write_text(text)
