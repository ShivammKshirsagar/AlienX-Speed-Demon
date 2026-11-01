use std::process::Command;

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
    // SAFETY: clock_gettime initializes the supplied timespec for CLOCK_MONOTONIC (1).
    assert_eq!(unsafe { clock_gettime(1, time.as_mut_ptr()) }, 0);
    // SAFETY: the successful clock_gettime call above initialized both timespec fields.
    let time = unsafe { time.assume_init() };
    time.seconds as u64 * 1_000_000_000 + time.nanoseconds as u64
}

fn value(line: &str, key: &str) -> u64 {
    line.split_whitespace()
        .find_map(|field| field.strip_prefix(key))
        .unwrap()
        .parse()
        .unwrap()
}

fn main() {
    let mut args = std::env::args_os().skip(1);
    let program = args.next().expect("child program required");
    let arguments: Vec<_> = args.collect();
    let launch_ns = monotonic_ns();
    let output = Command::new(program)
        .args(arguments)
        .env("PROFILE_LAUNCH_NS", launch_ns.to_string())
        .output()
        .expect("start profiled solution");
    let observed_exit_ns = monotonic_ns();
    let stderr = String::from_utf8_lossy(&output.stderr);
    print!("{}", String::from_utf8_lossy(&output.stdout));
    eprint!("{stderr}");
    if let Some(lifecycle) = stderr.lines().find(|line| line.starts_with("profile lifecycle ")) {
        let startup_ns = value(lifecycle, "main_start_ns=") - launch_ns;
        let exit_ns = observed_exit_ns - value(lifecycle, "exit_marker_ns=");
        eprintln!("wrapper lifecycle startup_ns={startup_ns} exit_ns={exit_ns} elapsed_ns={}", observed_exit_ns - launch_ns);
    } else {
        eprintln!("wrapper elapsed_ns={}", observed_exit_ns - launch_ns);
    }
    if !output.status.success() {
        std::process::exit(output.status.code().unwrap_or(1));
    }
}
