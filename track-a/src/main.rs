use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hash, Hasher};
use std::path::Path;
use std::thread;

const HOURS: usize = 72;
const MAX_THREADS: usize = 9;
const POST_PARSE_THREADS: usize = 5;
const INITIAL_ECAP: usize = 2048;

// A small, deterministic hasher is enough here: endpoint keys are short ASCII strings.
#[derive(Default)]
struct FastHasher(u64);

#[cfg(not(target_arch = "x86_64"))]
compile_error!("the pinned x86-64-v3 build requires x86_64");

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "sse4.2")]
unsafe fn crc32c_update(mut crc: u32, bytes: &[u8]) -> u32 {
    use std::arch::x86_64::{_mm_crc32_u64, _mm_crc32_u8};
    if bytes.len() == 1 {
        return _mm_crc32_u8(crc, bytes[0]);
    }
    let mut chunks = bytes.chunks_exact(8);
    for chunk in &mut chunks {
        let word = u64::from_ne_bytes(chunk.try_into().unwrap());
        crc = _mm_crc32_u64(u64::from(crc), word) as u32;
    }
    for &byte in chunks.remainder() {
        crc = _mm_crc32_u8(crc, byte);
    }
    crc
}

impl Hasher for FastHasher {
    fn write(&mut self, bytes: &[u8]) {
        #[cfg(target_arch = "x86_64")]
        let h = {
            // SAFETY: the shipped Docker build targets x86-64-v3, which includes SSE4.2.
            unsafe { crc32c_update(self.0 as u32, bytes) }
        };
        self.0 = u64::from(h) ^ (bytes.len() as u64).rotate_left(17);
    }

    fn finish(&self) -> u64 {
        let mut h = (self.0 ^ (self.0 >> 32)) as u32;
        h ^= h >> 16;
        h = h.wrapping_mul(0x7feb352d);
        h ^= h >> 15;
        h = h.wrapping_mul(0x846ca68b);
        h ^= h >> 16;
        u64::from(h) * 0x0000000100000001
    }
}

type FastMap<K, V> = HashMap<K, V, BuildHasherDefault<FastHasher>>;

#[derive(PartialEq, Eq)]
struct Endpoint<'a>(&'a [u8]);

impl Hash for Endpoint<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write(self.0);
    }
}

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

struct Local<'a> {
    endpoints: Vec<&'a [u8]>,
    endpoint_ids: FastMap<Endpoint<'a>, usize>,
    ecap: usize,
    group_ids: Vec<u32>,
    group_keys: Vec<u32>,
    groups: Vec<Group>,
}

impl<'a> Local<'a> {
    fn new() -> Self {
        Self {
            endpoints: Vec::new(),
            endpoint_ids: FastMap::default(),
            ecap: INITIAL_ECAP,
            group_ids: vec![u32::MAX; HOURS * INITIAL_ECAP],
            group_keys: Vec::new(),
            groups: Vec::new(),
        }
    }

    fn grow_ecap(&mut self) {
        let old = self.ecap;
        let new = old.checked_mul(2).expect("too many endpoint IDs");
        let mut group_ids = vec![u32::MAX; HOURS * new];
        for hour in 0..HOURS {
            group_ids[hour * new..hour * new + old]
                .copy_from_slice(&self.group_ids[hour * old..(hour + 1) * old]);
        }
        for key in &mut self.group_keys {
            let slot = *key as usize;
            *key = u32::try_from((slot / old) * new + slot % old).expect("group index exceeds u32");
        }
        self.group_ids = group_ids;
        self.ecap = new;
    }

    fn group_mut(&mut self, hour: usize, endpoint_id: usize) -> &mut Group {
        let slot = hour * self.ecap + endpoint_id;
        let mut group_id = self.group_ids[slot];
        if group_id == u32::MAX {
            group_id = u32::try_from(self.groups.len()).expect("too many groups");
            self.group_ids[slot] = group_id;
            self.group_keys
                .push(u32::try_from(slot).expect("group index exceeds u32"));
            self.groups.push(Group::new());
        }
        &mut self.groups[group_id as usize]
    }

    fn endpoint_id(&mut self, bytes: &'a [u8]) -> usize {
        let id = self.endpoints.len();
        if id == self.ecap {
            self.grow_ecap();
        }
        match self.endpoint_ids.entry(Endpoint(bytes)) {
            Entry::Occupied(entry) => *entry.get(),
            Entry::Vacant(entry) => {
                entry.insert(id);
                self.endpoints.push(bytes);
                id
            }
        }
    }
}

fn hour_index(ts: &[u8]) -> usize {
    let month = (ts[5] - b'0') * 10 + (ts[6] - b'0');
    let day = (ts[8] - b'0') * 10 + (ts[9] - b'0');
    let hour = (ts[11] - b'0') * 10 + (ts[12] - b'0');
    let day_offset = (month - 10) * 31 + day - 30;
    usize::from(day_offset) * 24 + usize::from(hour) - 18
}

fn parse_four(bytes: &[u8]) -> u32 {
    let digits = u32::from_le_bytes(bytes[..4].try_into().unwrap()) - 0x30303030;
    let pairs = (digits & 0x00ff00ff) * 10 + ((digits >> 8) & 0x00ff00ff);
    (pairs & 0xffff) * 100 + (pairs >> 16)
}

fn parse_uint(bytes: &[u8]) -> u32 {
    let (mut value, offset) = if bytes.len() >= 8 {
        (
            parse_four(&bytes[..4]) * 10_000 + parse_four(&bytes[4..8]),
            8,
        )
    } else if bytes.len() >= 4 {
        (parse_four(bytes), 4)
    } else {
        (0, 0)
    };
    for &byte in &bytes[offset..] {
        value = value * 10 + u32::from(byte - b'0');
    }
    value
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn find_line_fields_avx2(line: &[u8]) -> (usize, usize, usize) {
    use std::arch::x86_64::*;
    let (mut p, mut first, mut second, mut found) = (28, 0, 0, 0);
    while p + 32 <= line.len() {
        // SAFETY: the loop condition guarantees a full readable vector.
        let chunk = unsafe { _mm256_loadu_si256(line.as_ptr().add(p).cast()) };
        let spaces =
            _mm256_movemask_epi8(_mm256_cmpeq_epi8(chunk, _mm256_set1_epi8(b' ' as i8))) as u32;
        let newlines =
            _mm256_movemask_epi8(_mm256_cmpeq_epi8(chunk, _mm256_set1_epi8(b'\n' as i8))) as u32;
        let stop = if newlines == 0 {
            32
        } else {
            newlines.trailing_zeros() as usize
        };
        let mut mask = spaces
            & if stop == 32 {
                u32::MAX
            } else {
                (1 << stop) - 1
            };
        if mask != 0 && found == 0 {
            first = p + mask.trailing_zeros() as usize;
            found = 1;
            mask &= mask - 1;
        }
        if mask != 0 && found == 1 {
            second = p + mask.trailing_zeros() as usize;
            found = 2;
        }
        if newlines != 0 {
            return (p + stop, first, second);
        }
        p += 32;
    }
    for at in p..line.len() {
        if line[at] == b'\n' {
            return (at, first, second);
        }
        if line[at] == b' ' && found < 2 {
            if found == 0 {
                first = at;
            } else {
                second = at;
            }
            found += 1;
        }
    }
    (line.len(), first, second)
}

fn parse_range<'a>(data: &'a [u8], start: usize, end: usize) -> Local<'a> {
    let mut local = Local::new();
    let mut pos = start;
    parse_into(data, &mut pos, end, &mut local);
    assert_eq!(pos, end);
    local
}

fn parse_into<'a>(data: &'a [u8], pos: &mut usize, end: usize, local: &mut Local<'a>) {
    let mut previous_hour_key = u64::MAX;
    let mut previous_hour = 0;
    while *pos < end {
        // SAFETY: the release target uses x86-64-v3, and the scanner guards every vector load.
        let (line_len, field, endpoint_end) = unsafe { find_line_fields_avx2(&data[*pos..end]) };
        let line_end = *pos + line_len;
        let line = &data[*pos..line_end];
        let endpoint_start = field + 1;
        let status_start = endpoint_end + 1;
        let latency_start = status_start + 4;
        let endpoint = &line[endpoint_start..endpoint_end];
        let hour_key = u64::from_le_bytes(line[5..13].try_into().unwrap());
        if hour_key != previous_hour_key {
            previous_hour_key = hour_key;
            previous_hour = hour_index(line);
        }
        let hour = previous_hour;
        let endpoint_id = local.endpoint_id(endpoint);
        let group = local.group_mut(hour, endpoint_id);
        group.latencies.push(parse_uint(&line[latency_start..]));
        group.errors += u32::from(line[status_start] == b'5');
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

fn format_hours(
    range: std::ops::Range<usize>,
    capacity: usize,
    order: &[usize],
    endpoints: &[&[u8]],
    groups: &[Group],
    hours: &[[u8; 20]],
) -> Vec<u8> {
    let mut out = Vec::with_capacity(capacity);
    for hour in range {
        for &endpoint in order {
            let group = &groups[hour * endpoints.len() + endpoint];
            if group.latencies.is_empty() {
                continue;
            }
            out.extend_from_slice(&hours[hour]);
            out.push(b' ');
            out.extend_from_slice(endpoints[endpoint]);
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
    out
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

    let mut locals = thread::scope(|scope| {
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

    let workers = workers.min(POST_PARSE_THREADS);
    let mut global_ids: FastMap<&[u8], usize> = FastMap::default();
    let mut endpoints: Vec<&[u8]> = Vec::new();
    let mut remaps = Vec::with_capacity(locals.len());
    for local in &mut locals {
        let mut remap = Vec::with_capacity(local.endpoints.len());
        for endpoint in local.endpoints.drain(..) {
            let id = match global_ids.entry(endpoint) {
                Entry::Occupied(entry) => *entry.get(),
                Entry::Vacant(entry) => {
                    let id = endpoints.len();
                    entry.insert(id);
                    endpoints.push(endpoint);
                    id
                }
            };
            remap.push(id);
        }
        remaps.push(remap);
    }

    let range_size = HOURS.div_ceil(workers);
    let ranges: Vec<_> = (0..HOURS)
        .step_by(range_size)
        .map(|start| start..(start + range_size).min(HOURS))
        .collect();

    let endpoint_count = endpoints.len();
    let mut hour_to_range = vec![0usize; HOURS];
    for (index, range) in ranges.iter().enumerate() {
        for hour in range.clone() {
            hour_to_range[hour] = index;
        }
    }
    let mut range_jobs: Vec<Vec<(usize, Group)>> = (0..ranges.len()).map(|_| Vec::new()).collect();
    for (local, remap) in locals.into_iter().zip(remaps) {
        for (key, group) in local.group_keys.into_iter().zip(local.groups) {
            let key = key as usize;
            let hour = key / local.ecap;
            let endpoint = key % local.ecap;
            let range_index = hour_to_range[hour];
            let local_index = (hour - ranges[range_index].start) * endpoint_count + remap[endpoint];
            range_jobs[range_index].push((local_index, group));
        }
    }

    let mut groups: Vec<Group> = (0..endpoint_count * HOURS).map(|_| Group::new()).collect();
    thread::scope(|scope| {
        let mut remaining = groups.as_mut_slice();
        for (range, jobs) in ranges.into_iter().zip(range_jobs) {
            let (destination, tail) = remaining.split_at_mut(range.len() * endpoint_count);
            remaining = tail;
            scope.spawn(move || {
                for (index, mut partial) in jobs {
                    if destination[index].latencies.is_empty() {
                        destination[index].latencies = std::mem::take(&mut partial.latencies);
                    } else {
                        destination[index].latencies.append(&mut partial.latencies);
                    }
                    destination[index].errors += partial.errors;
                }
            });
        }
    });

    // Independent groups sort in parallel; sorting all values gives the three exact ranks.
    let target_work = groups
        .iter()
        .map(|group| group.latencies.len())
        .sum::<usize>()
        .div_ceil(workers)
        .max(1);
    let mut ranges = Vec::with_capacity(workers);
    let (mut start, mut work) = (0, 0);
    for (index, group) in groups.iter().enumerate() {
        work += group.latencies.len();
        if work >= target_work && ranges.len() + 1 < workers && index + 1 < groups.len() {
            ranges.push(start..index + 1);
            start = index + 1;
            work = 0;
        }
    }
    if start < groups.len() || ranges.is_empty() {
        ranges.push(start..groups.len());
    }
    // Contiguous ranges with similar record counts balance percentile-sort work.
    thread::scope(|scope| {
        let mut remaining = groups.as_mut_slice();
        for range in ranges {
            let (slice, tail) = remaining.split_at_mut(range.len());
            remaining = tail;
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
    order.sort_unstable_by(|&a, &b| endpoints[a].cmp(endpoints[b]));
    let hours = hour_strings();
    let mut hour_sizes = [0usize; HOURS];
    for hour in 0..HOURS {
        for &endpoint in &order {
            let group = &groups[hour * endpoints.len() + endpoint];
            if !group.latencies.is_empty() {
                hour_sizes[hour] += endpoints[endpoint].len() + 48;
            }
        }
    }
    let target_size = hour_sizes.iter().sum::<usize>().div_ceil(workers);
    let mut ranges = Vec::with_capacity(workers);
    let (mut start, mut size) = (0, 0);
    for (hour, &weight) in hour_sizes.iter().enumerate() {
        size += weight;
        if size >= target_size && ranges.len() + 1 < workers {
            ranges.push((start..hour + 1, size));
            start = hour + 1;
            size = 0;
        }
    }
    if start < HOURS || ranges.is_empty() {
        ranges.push((start..HOURS, size));
    }
    let output_buffers = thread::scope(|scope| {
        let joins = ranges
            .into_iter()
            .map(|(range, capacity)| {
                let order = &order;
                let endpoints = &endpoints;
                let groups = &groups;
                let hours = &hours;
                scope.spawn(move || format_hours(range, capacity, order, endpoints, groups, hours))
            })
            .collect::<Vec<_>>();
        joins
            .into_iter()
            .map(|join| join.join().expect("format worker panicked"))
            .collect::<Vec<_>>()
    });
    if let Some(parent) = Path::new(&output).parent() {
        std::fs::create_dir_all(parent).expect("create output directory");
    }
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(output)
        .expect("open output");
    use std::io::Write;
    for buffer in output_buffers {
        file.write_all(&buffer).expect("write output range");
    }
    drop(mapping);
    std::process::exit(0);
}
