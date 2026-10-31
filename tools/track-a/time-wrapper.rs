use std::process::Command;
use std::time::Instant;

fn main() {
    let start = Instant::now();
    let status = Command::new("/app/solution")
        .status()
        .expect("start solution");
    println!("process_ns={}", start.elapsed().as_nanos());
    let peak = std::fs::read_to_string("/sys/fs/cgroup/memory.peak")
        .expect("read cgroup memory.peak");
    println!("memory_peak_bytes={}", peak.trim());
    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }
}
