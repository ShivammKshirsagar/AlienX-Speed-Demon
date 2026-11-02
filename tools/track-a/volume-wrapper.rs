use std::process::{Command, ExitCode};
use std::thread;
use std::time::Duration;

#[repr(C)]
struct Timespec {
    seconds: std::os::raw::c_long,
    nanoseconds: std::os::raw::c_long,
}

fn monotonic_ns() -> u64 {
    unsafe extern "C" {
        fn clock_gettime(clock: i32, time: *mut Timespec) -> i32;
    }
    let mut time = std::mem::MaybeUninit::<Timespec>::uninit();
    // SAFETY: clock_gettime initializes this timespec for CLOCK_MONOTONIC (1).
    assert_eq!(unsafe { clock_gettime(1, time.as_mut_ptr()) }, 0);
    // SAFETY: the successful call above initialized both timespec fields.
    let time = unsafe { time.assume_init() };
    time.seconds as u64 * 1_000_000_000 + time.nanoseconds as u64
}

fn metric(path: &str, key: &str) -> u64 {
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .find_map(|line| {
            let mut fields = line.split_whitespace();
            (fields.next()? == key)
                .then(|| fields.next()?.parse().ok())
                .flatten()
        })
        .unwrap_or(0)
}

fn main() -> ExitCode {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let start = monotonic_ns();
    let mut child = Command::new("/app/solution")
        .args(&args)
        .spawn()
        .expect("start solution");
    let pid = child.id();
    let mut max_rss_kb = 0;
    let status = loop {
        if let Ok(text) = std::fs::read_to_string(format!("/proc/{pid}/status")) {
            for line in text.lines().filter(|line| line.starts_with("VmHWM:")) {
                max_rss_kb =
                    max_rss_kb.max(line.split_whitespace().nth(1).unwrap().parse().unwrap());
            }
        }
        if let Some(status) = child.try_wait().expect("wait for solution") {
            break status;
        }
        thread::sleep(Duration::from_micros(100));
    };
    let wall_ns = monotonic_ns() - start;
    let cpu_usec = metric("/sys/fs/cgroup/cpu.stat", "usage_usec");
    let memory_peak = std::fs::read_to_string("/sys/fs/cgroup/memory.peak")
        .ok()
        .and_then(|text| text.trim().parse().ok())
        .unwrap_or(0);
    println!(
        "VOLUME_METRICS wall_ns={wall_ns} cpu_usec={cpu_usec} vmhwm_kb={max_rss_kb} memory_peak_bytes={memory_peak}"
    );
    match status.code() {
        Some(0) => ExitCode::SUCCESS,
        Some(code) => ExitCode::from(code as u8),
        None => ExitCode::from(1),
    }
}
