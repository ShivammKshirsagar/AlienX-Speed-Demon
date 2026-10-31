use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};
use std::path::Path;
use std::thread;

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

fn parse_range(data: &[u8], start: usize, end: usize) -> Local {
    let mut local = Local::new();
    let mut pos = start;
    parse_into(data, &mut pos, end, &mut local);
    assert_eq!(pos, end);
    local
}

fn parse_into(data: &[u8], pos: &mut usize, end: usize, local: &mut Local) {
    while *pos < end {
        let line_end = data[*pos..end]
            .iter()
            .position(|&b| b == b'\n')
            .map_or(end, |n| *pos + n);
        let line = &data[*pos..line_end];

        // The timestamp has fixed width; only the date and hour bytes are needed.
        let mut field = 28;
        while line[field] != b' ' {
            field += 1;
        }
        let endpoint_start = field + 1;
        let endpoint_end = endpoint_start
            + line[endpoint_start..]
                .iter()
                .position(|&b| b == b' ')
                .unwrap();
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

struct Mapping {
    ptr: *mut u8,
    len: usize,
}

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
        let ptr = unsafe { mmap(std::ptr::null_mut(), len, 1, 2, file.as_raw_fd(), 0) };
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

fn main() {
    let mut args = std::env::args_os().skip(1);
    let input: std::path::PathBuf = args
        .next()
        .map_or_else(|| "/data/requests.log".into(), |s| s.into());
    let output: std::path::PathBuf = args
        .next()
        .map_or_else(|| "/out/summary.txt".into(), |s| s.into());
    let mapping = Mapping::open(Path::new(&input));
    let data = mapping.bytes();
    let workers = thread::available_parallelism()
        .map_or(1, usize::from)
        .min(MAX_THREADS)
        .min(data.len().max(1));
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

    let locals = thread::scope(|scope| {
        let mut joins = Vec::with_capacity(workers);
        for pair in bounds.windows(2) {
            let (start, end) = (pair[0], pair[1]);
            joins.push(scope.spawn(move || parse_range(data, start, end)));
        }
        joins
            .into_iter()
            .map(|j| j.join().expect("parser thread panicked"))
            .collect::<Vec<_>>()
    });

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

    // Independent groups sort in parallel; sorting all values gives the three exact ranks.
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

    let mut order: Vec<usize> = (0..endpoints.len()).collect();
    order.sort_unstable_by(|&a, &b| endpoints[a].cmp(&endpoints[b]));
    let hours = hour_strings();
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
    if let Some(parent) = Path::new(&output).parent() {
        std::fs::create_dir_all(parent).expect("create output directory");
    }
    std::fs::write(output, out).expect("write summary");
    drop(mapping);
    std::process::exit(0);
}
