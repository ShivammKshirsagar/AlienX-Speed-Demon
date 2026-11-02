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
        let mut chunks = bytes.chunks_exact(8);
        for chunk in &mut chunks {
            h = h.rotate_left(5) ^ u64::from_ne_bytes(chunk.try_into().unwrap());
        }
        let tail = chunks.remainder();
        if !tail.is_empty() {
            let mut last = [0; 8];
            last[..tail.len()].copy_from_slice(tail);
            h = h.rotate_left(5) ^ u64::from_ne_bytes(last);
        }
        self.0 = h ^ bytes.len() as u64;
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

struct Local<'a> {
    endpoints: Vec<&'a [u8]>,
    endpoint_ids: FastMap<&'a [u8], usize>,
    groups: Vec<FastMap<usize, Group>>,
}

impl<'a> Local<'a> {
    fn new() -> Self {
        Self {
            endpoints: Vec::new(),
            endpoint_ids: FastMap::default(),
            groups: (0..HOURS).map(|_| FastMap::default()).collect(),
        }
    }

    fn endpoint_id(&mut self, bytes: &'a [u8]) -> usize {
        if let Some(&id) = self.endpoint_ids.get(bytes) {
            return id;
        }
        let id = self.endpoints.len();
        self.endpoint_ids.insert(bytes, id);
        self.endpoints.push(bytes);
        id
    }
}

fn hour_index(ts: &[u8]) -> usize {
    let month = (ts[5] - b'0') * 10 + (ts[6] - b'0');
    let day = (ts[8] - b'0') * 10 + (ts[9] - b'0');
    let hour = (ts[11] - b'0') * 10 + (ts[12] - b'0');
    let day = usize::from(month - 10) * 31 + usize::from(day) - 30;
    day * 24 + usize::from(hour) - 18
}

fn parse_uint(bytes: &[u8]) -> u32 {
    bytes
        .iter()
        .fold(0u32, |n, &b| n * 10 + u32::from(b - b'0'))
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn find_line_fields_avx2(line: &[u8]) -> (usize, usize, usize) {
    use std::arch::x86_64::*;
    let (mut p, mut first, mut second, mut found) = (28, 0, 0, 0);
    while p + 64 <= line.len() {
        // SAFETY: p+64 is within the readable input slice, so both vector loads are in bounds.
        let (nl0, sp0, nl1, sp1) = unsafe {
            let a = _mm256_loadu_si256(line.as_ptr().add(p).cast());
            let b = _mm256_loadu_si256(line.as_ptr().add(p + 32).cast());
            (
                _mm256_movemask_epi8(_mm256_cmpeq_epi8(a, _mm256_set1_epi8(b'\n' as i8))) as u32,
                _mm256_movemask_epi8(_mm256_cmpeq_epi8(a, _mm256_set1_epi8(b' ' as i8))) as u32,
                _mm256_movemask_epi8(_mm256_cmpeq_epi8(b, _mm256_set1_epi8(b'\n' as i8))) as u32,
                _mm256_movemask_epi8(_mm256_cmpeq_epi8(b, _mm256_set1_epi8(b' ' as i8))) as u32,
            )
        };
        let newlines = u64::from(nl0) | (u64::from(nl1) << 32);
        let mut spaces = u64::from(sp0) | (u64::from(sp1) << 32);
        let stop = if newlines == 0 {
            64
        } else {
            newlines.trailing_zeros() as usize
        };
        if newlines != 0 {
            spaces &= (1u64 << stop) - 1;
        }
        if found < 2 && spaces != 0 {
            if found == 0 {
                first = p + spaces.trailing_zeros() as usize;
                found = 1;
                spaces &= spaces - 1;
            }
            if spaces != 0 && found == 1 {
                second = p + spaces.trailing_zeros() as usize;
                found = 2;
            }
        }
        if newlines != 0 {
            return (p + stop, first, second);
        }
        p += 64;
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

fn parse_uint_swar(bytes: &[u8]) -> u32 {
    if bytes.len() != 8 {
        return parse_uint(bytes);
    }
    // SAFETY: exactly eight token bytes are readable; converting from LE fixes byte order.
    let word = u64::from_le(unsafe { std::ptr::read_unaligned(bytes.as_ptr().cast()) });
    let mut value = 0u32;
    for shift in 0..8 {
        value = value * 10 + ((word >> (shift * 8)) as u8 - b'0') as u32;
    }
    value
}

fn parse_range<'a>(data: &'a [u8], start: usize, end: usize) -> Local<'a> {
    let mut local = Local::new();
    let mut pos = start;
    parse_into(data, &mut pos, end, &mut local);
    assert_eq!(pos, end);
    local
}

fn parse_into<'a>(data: &'a [u8], pos: &mut usize, end: usize, local: &mut Local<'a>) {
    while *pos < end {
        // SAFETY: the pinned release target supports AVX2 and the scanner guards every load.
        let (line_len, field, endpoint_end) = unsafe { find_line_fields_avx2(&data[*pos..end]) };
        let line_end = *pos + line_len;
        let line = &data[*pos..line_end];
        let endpoint_start = field + 1;
        let status_start = endpoint_end + 1;
        let latency_start = status_start + 4;
        let endpoint = &line[endpoint_start..endpoint_end];
        let hour = hour_index(line);
        let endpoint_id = local.endpoint_id(endpoint);
        let group = local.groups[hour]
            .entry(endpoint_id)
            .or_insert_with(Group::new);
        group
            .latencies
            .push(parse_uint_swar(&line[latency_start..]));
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

    let mut global_ids: FastMap<&[u8], usize> = FastMap::default();
    let mut endpoints: Vec<&[u8]> = Vec::new();
    let mut remaps = Vec::with_capacity(locals.len());
    for local in &mut locals {
        let mut remap = Vec::with_capacity(local.endpoints.len());
        for endpoint in local.endpoints.drain(..) {
            let id = if let Some(&id) = global_ids.get(endpoint) {
                id
            } else {
                let id = endpoints.len();
                global_ids.insert(endpoint, id);
                endpoints.push(endpoint);
                id
            };
            remap.push(id);
        }
        remaps.push(remap);
    }

    let hour_weights: Vec<usize> = (0..HOURS)
        .map(|hour| {
            locals
                .iter()
                .map(|local| {
                    local.groups[hour]
                        .values()
                        .map(|group| group.latencies.len())
                        .sum::<usize>()
                })
                .sum()
        })
        .collect();
    let target_hour_work = hour_weights.iter().sum::<usize>().div_ceil(workers).max(1);
    let mut ranges = Vec::with_capacity(workers);
    let (mut start_hour, mut hour_work) = (0, 0);
    for (hour, &weight) in hour_weights.iter().enumerate() {
        hour_work += weight;
        if hour_work >= target_hour_work && ranges.len() + 1 < workers {
            ranges.push(start_hour..hour + 1);
            start_hour = hour + 1;
            hour_work = 0;
        }
    }
    if start_hour < HOURS || ranges.is_empty() {
        ranges.push(start_hour..HOURS);
    }

    let mut range_jobs: Vec<Vec<(Vec<usize>, Vec<FastMap<usize, Group>>)>> = (0..ranges.len())
        .map(|_| Vec::with_capacity(locals.len()))
        .collect();
    for (local, remap) in locals.into_iter().zip(remaps) {
        let mut hour_maps = local.groups.into_iter();
        for (range, jobs) in ranges.iter().zip(&mut range_jobs) {
            let maps = hour_maps.by_ref().take(range.len()).collect();
            jobs.push((remap.clone(), maps));
        }
    }

    let endpoint_count = endpoints.len();
    let mut groups: Vec<Group> = (0..endpoint_count * HOURS).map(|_| Group::new()).collect();
    thread::scope(|scope| {
        let mut remaining = groups.as_mut_slice();
        for (range, sources) in ranges.into_iter().zip(range_jobs) {
            let (destination, tail) = remaining.split_at_mut(range.len() * endpoint_count);
            remaining = tail;
            scope.spawn(move || {
                for (remap, hour_maps) in sources {
                    for (hour_offset, groups) in hour_maps.into_iter().enumerate() {
                        for (local_endpoint, mut partial) in groups {
                            let index = hour_offset * endpoint_count + remap[local_endpoint];
                            if destination[index].latencies.is_empty() {
                                destination[index].latencies =
                                    std::mem::take(&mut partial.latencies);
                            } else {
                                destination[index].latencies.append(&mut partial.latencies);
                            }
                            destination[index].errors += partial.errors;
                        }
                    }
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
    let file = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(output)
        .expect("open output");
    let total_size = output_buffers.iter().map(Vec::len).sum::<usize>();
    file.set_len(total_size as u64).expect("size output");
    let mut offset = 0u64;
    let offsets = output_buffers
        .iter()
        .map(|buffer| {
            let start = offset;
            offset += buffer.len() as u64;
            start
        })
        .collect::<Vec<_>>();
    use std::os::unix::fs::FileExt;
    thread::scope(|scope| {
        for (buffer, offset) in output_buffers.into_iter().zip(offsets) {
            let file = &file;
            scope.spawn(move || {
                file.write_all_at(&buffer, offset)
                    .expect("write output range");
            });
        }
    });
    unsafe extern "C" {
        fn _exit(status: i32) -> !;
    }
    // SAFETY: output workers have joined and no destructor is needed after the write.
    unsafe { _exit(0) }
}
