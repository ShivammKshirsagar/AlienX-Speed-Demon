use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};
use std::path::Path;
use std::thread;
use std::time::Instant;

const HOURS: usize = 72;
const MAX_THREADS: usize = 12;

// A small, deterministic hasher is enough here: endpoint keys are short ASCII strings.
#[derive(Default)]
struct FastHasher(u64);

impl Hasher for FastHasher {
    fn write(&mut self, bytes: &[u8]) {
        let mut h = if self.0 == 0 {
            0xcbf29ce484222325
        } else {
            self.0
        };
        for &b in bytes {
            h = (h ^ u64::from(b)).wrapping_mul(0x100000001b3);
        }
        self.0 = h;
    }

    fn finish(&self) -> u64 {
        self.0
    }
}

type FastMap<K, V> = HashMap<K, V, BuildHasherDefault<FastHasher>>;

struct Group {
    latencies: Vec<u32>,
    errors: u32,
    p50: u32,
    p95: u32,
    p99: u32,
}

impl Group {
    fn new() -> Self {
        Self {
            latencies: Vec::new(),
            errors: 0,
            p50: 0,
            p95: 0,
            p99: 0,
        }
    }
}

struct Local {
    endpoints: Vec<Vec<u8>>,
    endpoint_ids: FastMap<Vec<u8>, usize>,
    groups: FastMap<(u8, usize), Group>,
}

impl Local {
    fn new() -> Self {
        Self {
            endpoints: Vec::new(),
            endpoint_ids: FastMap::default(),
            groups: FastMap::default(),
        }
    }

    fn endpoint_id(&mut self, bytes: &[u8]) -> usize {
        if let Some(&id) = self.endpoint_ids.get(bytes) {
            return id;
        }
        let id = self.endpoints.len();
        let owned = bytes.to_vec();
        self.endpoint_ids.insert(owned.clone(), id);
        self.endpoints.push(owned);
        id
    }
}

fn hour_index(ts: &[u8]) -> usize {
    let month = (ts[5] - b'0') * 10 + (ts[6] - b'0');
    let day = (ts[8] - b'0') * 10 + (ts[9] - b'0');
    let hour = (ts[11] - b'0') * 10 + (ts[12] - b'0');
    let day_offset = match (month, day) {
        (10, 30) => 0,
        (10, 31) => 24,
        (11, 1) => 48,
        (11, 2) => 72,
        _ => unreachable!("timestamp outside the specified window"),
    };
    day_offset + usize::from(hour) - 18
}

fn parse_uint(bytes: &[u8]) -> u32 {
    bytes
        .iter()
        .fold(0u32, |n, &b| n * 10 + u32::from(b - b'0'))
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn find_byte_avx2(bytes: &[u8], needle: u8) -> Option<usize> {
    use std::arch::x86_64::*;

    let target = _mm256_set1_epi8(needle as i8);
    let mut offset = 0;
    while offset + 32 <= bytes.len() {
        // SAFETY: the loop condition guarantees 32 readable bytes remain in the slice.
        let chunk = unsafe { _mm256_loadu_si256(bytes.as_ptr().add(offset).cast()) };
        let matches = _mm256_movemask_epi8(_mm256_cmpeq_epi8(chunk, target)) as u32;
        if matches != 0 {
            return Some(offset + matches.trailing_zeros() as usize);
        }
        offset += 32;
    }
    bytes[offset..]
        .iter()
        .position(|&byte| byte == needle)
        .map(|index| offset + index)
}

#[inline]
fn find_byte(bytes: &[u8], needle: u8) -> Option<usize> {
    #[cfg(target_arch = "x86_64")]
    if std::arch::is_x86_feature_detected!("avx2") {
        // SAFETY: the runtime feature check proves AVX2 is available on this CPU.
        return unsafe { find_byte_avx2(bytes, needle) };
    }
    bytes.iter().position(|&byte| byte == needle)
}

fn parse_range(data: &[u8], mut pos: usize, end: usize) -> Local {
    let mut local = Local::new();
    parse_into(data, &mut pos, end, &mut local);
    local
}

fn parse_slice(data: &[u8], local: &mut Local) {
    let mut pos = 0;
    parse_into(data, &mut pos, data.len(), local);
    assert_eq!(pos, data.len());
}

fn parse_into(data: &[u8], pos: &mut usize, end: usize, local: &mut Local) {
    while *pos < end {
        let line_end = find_byte(&data[*pos..end], b'\n').map_or(end, |n| *pos + n);
        let line = &data[*pos..line_end];

        // The timestamp has fixed width; only the date and hour bytes are needed.
        let field = 28 + find_byte(&line[28..], b' ').unwrap();
        let endpoint_start = field + 1;
        let endpoint_end = endpoint_start + find_byte(&line[endpoint_start..], b' ').unwrap();
        let status_start = endpoint_end + 1;
        let latency_start = status_start + 4;
        let endpoint = &line[endpoint_start..endpoint_end];
        let hour = hour_index(line);
        let endpoint_id = local.endpoint_id(endpoint);
        let group = local
            .groups
            .entry((hour as u8, endpoint_id))
            .or_insert_with(Group::new);
        group.latencies.push(parse_uint(&line[latency_start..]));
        if line[status_start] == b'5' {
            group.errors += 1;
        }
        *pos = line_end + 1;
    }
}

#[cfg(unix)]
fn parse_pread_range(path: &Path, start: usize, end: usize, chunk_size: usize) -> Local {
    use std::os::unix::fs::FileExt;

    let file = std::fs::File::open(path).expect("open pread input");
    let mut local = Local::new();
    let mut buffer = vec![0u8; chunk_size];
    let mut buffered = 0usize;
    let mut offset = start as u64;
    while offset < end as u64 {
        let capacity = buffer.len() - buffered;
        let requested = capacity.min((end as u64 - offset) as usize);
        let read = file
            .read_at(&mut buffer[buffered..buffered + requested], offset)
            .expect("pread input");
        assert!(
            read > 0 && read <= requested,
            "short pread before assigned range end"
        );
        buffered += read;
        offset += read as u64;

        let complete = if offset == end as u64 {
            buffered
        } else {
            buffer[..buffered]
                .iter()
                .rposition(|&b| b == b'\n')
                .map_or(0, |index| index + 1)
        };
        if complete != 0 {
            parse_slice(&buffer[..complete], &mut local);
            buffer.copy_within(complete..buffered, 0);
            buffered -= complete;
        }
        assert!(buffered < 256, "line exceeded the documented maximum size");
    }
    assert_eq!(buffered, 0, "assigned range ended mid-line");
    local
}

#[cfg(unix)]
struct Mapping {
    ptr: *mut u8,
    len: usize,
}

#[cfg(unix)]
impl Mapping {
    fn open(path: &Path) -> Self {
        use std::os::unix::io::AsRawFd;
        let file = std::fs::File::open(path).expect("open input");
        let len = file.metadata().expect("input metadata").len() as usize;
        if len == 0 {
            return Self {
                ptr: std::ptr::NonNull::<u8>::dangling().as_ptr(),
                len,
            };
        }
        unsafe extern "C" {
            fn mmap(
                addr: *mut std::ffi::c_void,
                length: usize,
                prot: i32,
                flags: i32,
                fd: i32,
                offset: isize,
            ) -> *mut std::ffi::c_void;
        }
        // SAFETY: the read-only mapping remains valid after the descriptor closes, and the
        // returned pointer is checked against MAP_FAILED before it is accessed.
        let flags = if std::env::var_os("PROFILE_MAP_POPULATE").is_some() {
            2 | 0x8000
        } else {
            2
        };
        let ptr = unsafe { mmap(std::ptr::null_mut(), len, 1, flags, file.as_raw_fd(), 0) };
        assert_ne!(ptr as isize, -1, "mmap input");
        Self {
            ptr: ptr.cast(),
            len,
        }
    }

    fn bytes(&self) -> &[u8] {
        // SAFETY: mmap returned a readable region of exactly `len` bytes, held until Drop.
        unsafe { std::slice::from_raw_parts(self.ptr, self.len) }
    }
}

#[cfg(unix)]
impl Drop for Mapping {
    fn drop(&mut self) {
        unsafe extern "C" {
            fn munmap(addr: *mut std::ffi::c_void, length: usize) -> i32;
        }
        // SAFETY: this is the unchanged address and length returned by mmap.
        if self.len != 0 {
            // SAFETY: this is the unchanged address and length returned by mmap.
            unsafe {
                munmap(self.ptr.cast(), self.len);
            }
        }
    }
}

#[cfg(windows)]
struct Mapping {
    ptr: *mut u8,
    len: usize,
    mapping: std::os::windows::io::RawHandle,
}

#[cfg(windows)]
#[link(name = "kernel32")]
unsafe extern "system" {
    fn CreateFileMappingW(
        file: std::os::windows::io::RawHandle,
        attrs: *mut std::ffi::c_void,
        protect: u32,
        max_high: u32,
        max_low: u32,
        name: *const u16,
    ) -> std::os::windows::io::RawHandle;
    fn MapViewOfFile(
        mapping: std::os::windows::io::RawHandle,
        access: u32,
        offset_high: u32,
        offset_low: u32,
        bytes: usize,
    ) -> *mut std::ffi::c_void;
    fn UnmapViewOfFile(address: *const std::ffi::c_void) -> i32;
    fn CloseHandle(handle: std::os::windows::io::RawHandle) -> i32;
}

#[cfg(windows)]
impl Mapping {
    fn open(path: &Path) -> Self {
        use std::os::windows::io::AsRawHandle;
        let file = std::fs::File::open(path).expect("open input");
        let len = file.metadata().expect("input metadata").len() as usize;
        if len == 0 {
            return Self {
                ptr: std::ptr::NonNull::<u8>::dangling().as_ptr(),
                len,
                mapping: std::ptr::null_mut(),
            };
        }
        // SAFETY: `file` is a valid open handle and the mapping is requested read-only.
        let mapping = unsafe {
            CreateFileMappingW(
                file.as_raw_handle(),
                std::ptr::null_mut(),
                2,
                0,
                0,
                std::ptr::null(),
            )
        };
        assert!(!mapping.is_null(), "CreateFileMappingW input");
        // SAFETY: `mapping` is a live handle returned above; offset zero and size zero map the file.
        let ptr = unsafe { MapViewOfFile(mapping, 4, 0, 0, 0) };
        assert!(!ptr.is_null(), "MapViewOfFile input");
        Self {
            ptr: ptr.cast(),
            len,
            mapping,
        }
    }

    fn bytes(&self) -> &[u8] {
        // SAFETY: MapViewOfFile exposes the complete read-only file view for this lifetime.
        unsafe { std::slice::from_raw_parts(self.ptr, self.len) }
    }
}

#[cfg(windows)]
impl Drop for Mapping {
    fn drop(&mut self) {
        if self.len != 0 {
            // SAFETY: these are the view and mapping handle returned by the Windows APIs.
            unsafe {
                UnmapViewOfFile(self.ptr.cast());
                CloseHandle(self.mapping);
            }
        }
    }
}

#[cfg(not(any(unix, windows)))]
compile_error!("the solution requires a Unix or Windows memory mapping API");

fn hour_strings() -> Vec<[u8; 20]> {
    (0..HOURS)
        .map(|idx| {
            let absolute = idx + 18;
            let day = absolute / 24;
            let hour = absolute % 24;
            let (month, date) = match day {
                0 => (10, 30),
                1 => (10, 31),
                2 => (11, 1),
                _ => (11, 2),
            };
            let mut out = [0u8; 20];
            let value = format!("2026-{month:02}-{date:02}T{hour:02}:00:00Z");
            out.copy_from_slice(value.as_bytes());
            out
        })
        .collect()
}

fn push_uint(out: &mut Vec<u8>, mut n: u64) {
    let mut digits = [0u8; 20];
    let mut at = digits.len();
    loop {
        at -= 1;
        digits[at] = b'0' + (n % 10) as u8;
        n /= 10;
        if n == 0 {
            break;
        }
    }
    out.extend_from_slice(&digits[at..]);
}

fn page_faults() -> (u64, u64) {
    let stat = std::fs::read_to_string("/proc/self/stat").unwrap();
    let fields: Vec<&str> = stat[stat.rfind(')').unwrap() + 2..]
        .split_whitespace()
        .collect();
    (fields[7].parse().unwrap(), fields[9].parse().unwrap())
}

#[derive(Clone, Copy)]
struct Usage {
    user_us: u64,
    system_us: u64,
    minor_faults: u64,
    major_faults: u64,
    high_water_kb: u64,
}

#[repr(C)]
struct TimeValue {
    seconds: std::os::raw::c_long,
    microseconds: std::os::raw::c_long,
}

#[repr(C)]
struct RawUsage {
    user: TimeValue,
    system: TimeValue,
    fields: [std::os::raw::c_long; 14],
}

fn usage() -> Usage {
    unsafe extern "C" {
        fn getrusage(who: i32, usage: *mut RawUsage) -> i32;
    }
    let mut raw = std::mem::MaybeUninit::<RawUsage>::uninit();
    // SAFETY: getrusage initializes the supplied RawUsage when called with RUSAGE_SELF (0).
    assert_eq!(unsafe { getrusage(0, raw.as_mut_ptr()) }, 0, "getrusage");
    // SAFETY: the successful getrusage call above initialized every field.
    let raw = unsafe { raw.assume_init() };
    let micros = |time: TimeValue| (time.seconds as u64) * 1_000_000 + time.microseconds as u64;
    Usage {
        user_us: micros(raw.user),
        system_us: micros(raw.system),
        minor_faults: raw.fields[4] as u64,
        major_faults: raw.fields[5] as u64,
        high_water_kb: raw.fields[0] as u64,
    }
}

fn report_usage(phase: &str, start: Usage, end: Usage) {
    eprintln!(
        "profile rusage phase={phase} user_us={} system_us={} minor_faults={} major_faults={} cumulative_hwm_kb={}",
        end.user_us - start.user_us,
        end.system_us - start.system_us,
        end.minor_faults - start.minor_faults,
        end.major_faults - start.major_faults,
        end.high_water_kb
    );
}

fn rss_bytes(field: &str) -> u64 {
    std::fs::read_to_string("/proc/self/status")
        .unwrap()
        .lines()
        .find_map(|line| line.strip_prefix(field))
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap()
        .parse::<u64>()
        .unwrap()
        * 1024
}

fn rss_current() -> u64 {
    rss_bytes("VmRSS:")
}

fn rss_peak() -> u64 {
    rss_bytes("VmHWM:")
}

#[cfg(unix)]
fn apply_madvise(mapping: &Mapping) -> u128 {
    let Ok(mode) = std::env::var("PROFILE_ADVICE") else {
        return 0;
    };
    let advice = match mode.as_str() {
        "sequential" => 2,
        "willneed" => 3,
        _ => return 0,
    };
    unsafe extern "C" {
        fn madvise(addr: *mut std::ffi::c_void, length: usize, advice: i32) -> i32;
    }
    let start = Instant::now();
    // SAFETY: `mapping` is the valid read-only mmap region and the advice only affects readahead.
    let result = unsafe { madvise(mapping.ptr.cast(), mapping.len, advice) };
    assert_eq!(result, 0, "madvise input");
    start.elapsed().as_nanos()
}

fn main() {
    let total_start = Instant::now();
    let usage_start = usage();
    let faults_start = page_faults();
    let mut args = std::env::args_os().skip(1);
    let input: std::path::PathBuf = args
        .next()
        .map_or_else(|| "/data/requests.log".into(), |s| s.into());
    let output: std::path::PathBuf = args
        .next()
        .map_or_else(|| "/out/summary.txt".into(), |s| s.into());
    let mmap_start = Instant::now();
    let mapping = Mapping::open(Path::new(&input));
    let data = mapping.bytes();
    let mmap_ns = mmap_start.elapsed().as_nanos();
    let usage_after_map = usage();
    let madvise_ns = apply_madvise(&mapping);
    let faults_after_map = page_faults();
    let memory_after_map = rss_current();
    let available = thread::available_parallelism().map_or(1, usize::from);
    let requested = std::env::var("PROFILE_THREADS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(MAX_THREADS)
        .max(1);
    let workers = available
        .min(requested)
        .min(MAX_THREADS)
        .min(data.len().max(1));
    let boundary_start = Instant::now();
    let mut bounds = Vec::with_capacity(workers + 1);
    bounds.push(0);
    for i in 1..workers {
        let mut p = data.len() * i / workers;
        while p < data.len() && data[p - 1] != b'\n' {
            p += 1;
        }
        bounds.push(p);
    }
    bounds.push(data.len());
    let boundary_ns = boundary_start.elapsed().as_nanos();
    let usage_after_bounds = usage();
    let memory_after_bounds = rss_current();

    let parse_start = Instant::now();
    let parse_origin = parse_start;
    let use_pread = std::env::var_os("PROFILE_PREAD").is_some();
    let chunk_size = std::env::var("PROFILE_CHUNK_SIZE")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(1 << 20)
        .max(256);
    let input_path = input.as_path();
    let (locals, spawn_ns, worker_times) = thread::scope(|scope| {
        let mut joins = Vec::with_capacity(workers);
        let spawning = Instant::now();
        for pair in bounds.windows(2) {
            let (start, end) = (pair[0], pair[1]);
            joins.push(scope.spawn(move || {
                let started = parse_origin.elapsed().as_nanos();
                let local = if use_pread {
                    parse_pread_range(input_path, start, end, chunk_size)
                } else {
                    parse_range(data, start, end)
                };
                let finished = parse_origin.elapsed().as_nanos();
                (local, started, finished)
            }));
        }
        let spawn_ns = spawning.elapsed().as_nanos();
        let joined = joins
            .into_iter()
            .map(|j| j.join().expect("parser thread panicked"))
            .collect::<Vec<_>>();
        let worker_times = joined
            .iter()
            .map(|(_, started, finished)| (*started, *finished))
            .collect::<Vec<_>>();
        let locals = joined
            .into_iter()
            .map(|(local, _, _)| local)
            .collect::<Vec<_>>();
        (locals, spawn_ns, worker_times)
    });
    let parse_ns = parse_start.elapsed().as_nanos();
    let usage_after_parse = usage();
    let faults_after_parse = page_faults();
    let memory_after_parse = rss_current();

    let merge_start = Instant::now();
    let mut global_ids: FastMap<Vec<u8>, usize> = FastMap::default();
    let mut endpoints: Vec<Vec<u8>> = Vec::new();
    let mut groups: Vec<Group> = Vec::new();
    for mut local in locals {
        let mut remap = Vec::with_capacity(local.endpoints.len());
        for endpoint in local.endpoints.drain(..) {
            let id = if let Some(&id) = global_ids.get(endpoint.as_slice()) {
                id
            } else {
                let id = endpoints.len();
                global_ids.insert(endpoint.clone(), id);
                endpoints.push(endpoint);
                groups.resize_with(endpoints.len() * HOURS, Group::new);
                id
            };
            remap.push(id);
        }
        for ((hour, local_id), mut local_group) in local.groups.drain() {
            let global = remap[local_id] * HOURS + usize::from(hour);
            groups[global].latencies.append(&mut local_group.latencies);
            groups[global].errors += local_group.errors;
        }
    }
    let merge_ns = merge_start.elapsed().as_nanos();
    let usage_after_merge = usage();
    let memory_after_merge = rss_current();

    // Independent groups sort in parallel; sorting all values gives the three exact ranks.
    let selection_start = Instant::now();
    let chunks = (groups.len() + workers - 1) / workers;
    thread::scope(|scope| {
        for slice in groups.chunks_mut(chunks.max(1)) {
            scope.spawn(move || {
                for group in slice.iter_mut().filter(|g| !g.latencies.is_empty()) {
                    group.latencies.sort_unstable();
                    let n = group.latencies.len();
                    group.p50 = group.latencies[(50 * n + 99) / 100 - 1];
                    group.p95 = group.latencies[(95 * n + 99) / 100 - 1];
                    group.p99 = group.latencies[(99 * n + 99) / 100 - 1];
                }
            });
        }
    });
    let selection_ns = selection_start.elapsed().as_nanos();
    let usage_after_selection = usage();
    let memory_after_selection = rss_current();

    let endpoint_sort_start = Instant::now();
    let mut order: Vec<usize> = (0..endpoints.len()).collect();
    order.sort_unstable_by(|&a, &b| endpoints[a].cmp(&endpoints[b]));
    let hours = hour_strings();
    let endpoint_sort_ns = endpoint_sort_start.elapsed().as_nanos();
    let usage_after_endpoint_sort = usage();

    let output_format_start = Instant::now();
    let mut out = Vec::with_capacity(10_500_000);
    for hour in 0..HOURS {
        for &endpoint in &order {
            let group = &groups[endpoint * HOURS + hour];
            if group.latencies.is_empty() {
                continue;
            }
            out.extend_from_slice(&hours[hour]);
            out.push(b' ');
            out.extend_from_slice(&endpoints[endpoint]);
            out.push(b' ');
            let count = group.latencies.len() as u64;
            push_uint(&mut out, count);
            out.push(b' ');
            push_uint(&mut out, u64::from(group.p50));
            out.push(b' ');
            push_uint(&mut out, u64::from(group.p95));
            out.push(b' ');
            push_uint(&mut out, u64::from(group.p99));
            out.push(b' ');
            push_uint(&mut out, u64::from(group.errors));
            out.push(b' ');
            push_uint(&mut out, u64::from(group.errors) * 1_000_000 / count);
            out.push(b'\n');
        }
    }
    let output_format_ns = output_format_start.elapsed().as_nanos();
    let usage_after_output = usage();
    let memory_after_output = rss_current();
    if let Some(parent) = Path::new(&output).parent() {
        std::fs::create_dir_all(parent).expect("create output directory");
    }
    let write_start = Instant::now();
    std::fs::write(output, out).expect("write summary");
    let write_ns = write_start.elapsed().as_nanos();
    let usage_after_write = usage();
    let memory_after_write = rss_current();
    let faults_end = page_faults();
    eprintln!("profile mmap_ns={mmap_ns} madvise_ns={madvise_ns} boundary_ns={boundary_ns} spawn_ns={spawn_ns} parse_wall_ns={parse_ns} merge_ns={merge_ns} scatter_ns=0 selection_ns={selection_ns} endpoint_sort_ns={endpoint_sort_ns} output_format_ns={output_format_ns} write_ns={write_ns} total_main_ns={}", total_start.elapsed().as_nanos());
    eprintln!("profile workers_started_finished_ns={worker_times:?}");
    eprintln!("profile faults_minor_major start={faults_start:?} after_map={faults_after_map:?} after_parse={faults_after_parse:?} end={faults_end:?}");
    eprintln!("profile rss_current_bytes after_map={memory_after_map} after_bounds={memory_after_bounds} after_parse={memory_after_parse} after_merge={memory_after_merge} after_selection={memory_after_selection} after_output={memory_after_output} after_write={memory_after_write}");
    eprintln!("profile rss_peak_bytes={}", rss_peak());
    report_usage("mmap", usage_start, usage_after_map);
    report_usage("boundary", usage_after_map, usage_after_bounds);
    report_usage("parse", usage_after_bounds, usage_after_parse);
    report_usage("merge", usage_after_parse, usage_after_merge);
    report_usage("selection", usage_after_merge, usage_after_selection);
    report_usage(
        "endpoint_sort",
        usage_after_selection,
        usage_after_endpoint_sort,
    );
    report_usage(
        "output_format",
        usage_after_endpoint_sort,
        usage_after_output,
    );
    report_usage("write", usage_after_output, usage_after_write);
    drop(mapping);
    std::process::exit(0);
}
