use std::path::Path;
use std::thread;

const HOURS: usize = 72;
const MAX_THREADS: usize = 12;
const INITIAL_ECAP: usize = 2048;

fn endpoint_hash(bytes: &[u8]) -> u64 {
    fn word(bytes: &[u8]) -> u64 {
        let mut padded = [0u8; 8];
        padded[..bytes.len()].copy_from_slice(bytes);
        u64::from_ne_bytes(padded)
    }
    let mut hash = (bytes.len() as u64).wrapping_mul(0x9e3779b185ebca87);
    hash ^= word(&bytes[..bytes.len().min(8)]);
    hash = hash.wrapping_mul(0xc2b2ae3d27d4eb4f);
    if bytes.len() > 8 {
        hash ^= word(&bytes[8..bytes.len().min(16)]);
        hash = hash.wrapping_mul(0x165667b19e3779f9);
    }
    hash ^ (hash >> 32)
}

#[derive(Clone, Copy)]
struct Slot<'a> {
    hash: u64,
    key: Option<&'a [u8]>,
    id: usize,
}

struct Local<'a> {
    endpoints: Vec<&'a [u8]>,
    endpoint_slots: Vec<Slot<'a>>,
    endpoint_count: usize,
    ecap: usize,
    counts: Vec<u32>,
    errors: Vec<u32>,
    recs: Vec<u64>,
}

impl<'a> Local<'a> {
    fn new(chunk_len: usize) -> Self {
        let ecap = INITIAL_ECAP;
        Self {
            endpoints: Vec::new(),
            endpoint_slots: vec![
                Slot {
                    hash: 0,
                    key: None,
                    id: 0
                };
                4096
            ],
            endpoint_count: 0,
            ecap,
            counts: vec![0; HOURS * ecap],
            errors: vec![0; HOURS * ecap],
            recs: Vec::with_capacity(chunk_len / 64),
        }
    }

    fn grow_ecap(&mut self) {
        let old = self.ecap;
        let new = old.checked_mul(2).expect("too many endpoints in chunk");
        let mut counts = vec![0; HOURS * new];
        let mut errors = vec![0; HOURS * new];
        for hour in 0..HOURS {
            counts[hour * new..hour * new + old]
                .copy_from_slice(&self.counts[hour * old..(hour + 1) * old]);
            errors[hour * new..hour * new + old]
                .copy_from_slice(&self.errors[hour * old..(hour + 1) * old]);
        }
        // Record keys are hour * ECAP + local id; rewrite them when the stride changes.
        for rec in &mut self.recs {
            let old_key = (*rec >> 32) as usize;
            let hour = old_key / old;
            let local_id = old_key % old;
            *rec = (((hour * new + local_id) as u64) << 32) | (*rec & u64::from(u32::MAX));
        }
        self.counts = counts;
        self.errors = errors;
        self.ecap = new;
    }

    fn endpoint_id(&mut self, bytes: &'a [u8]) -> usize {
        let hash = endpoint_hash(bytes);
        let mut index = hash as usize & (self.endpoint_slots.len() - 1);
        loop {
            let slot = self.endpoint_slots[index];
            match slot.key {
                Some(key) if slot.hash == hash && key == bytes => {
                    return slot.id;
                }
                Some(_) => index = (index + 1) & (self.endpoint_slots.len() - 1),
                None => {
                    if (self.endpoint_count + 1) * 10 >= self.endpoint_slots.len() * 7 {
                        self.grow_endpoint_table();
                        return self.endpoint_id(bytes);
                    }
                    let id = self.endpoints.len();
                    if id == self.ecap {
                        self.grow_ecap();
                    }
                    self.endpoint_slots[index] = Slot {
                        hash,
                        key: Some(bytes),
                        id,
                    };
                    self.endpoint_count += 1;
                    self.endpoints.push(bytes);
                    return id;
                }
            }
        }
    }

    fn grow_endpoint_table(&mut self) {
        let capacity = self.endpoint_slots.len() * 2;
        let old = std::mem::replace(
            &mut self.endpoint_slots,
            vec![
                Slot {
                    hash: 0,
                    key: None,
                    id: 0
                };
                capacity
            ],
        );
        for slot in old.into_iter().filter(|slot| slot.key.is_some()) {
            let mut index = slot.hash as usize & (self.endpoint_slots.len() - 1);
            while self.endpoint_slots[index].key.is_some() {
                index = (index + 1) & (self.endpoint_slots.len() - 1);
            }
            self.endpoint_slots[index] = slot;
        }
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
    #[cfg(all(target_arch = "x86_64", target_feature = "avx2"))]
    {
        // SAFETY: the release target is explicitly compiled with AVX2 enabled.
        return unsafe { find_byte_avx2(bytes, needle) };
    }
    #[cfg(not(all(target_arch = "x86_64", target_feature = "avx2")))]
    bytes.iter().position(|&byte| byte == needle)
}

fn parse_range<'a>(data: &'a [u8], start: usize, end: usize) -> Local<'a> {
    let mut local = Local::new(end - start);
    let mut pos = start;
    while pos < end {
        let line_end = find_byte(&data[pos..end], b'\n').map_or(end, |n| pos + n);
        let line = &data[pos..line_end];

        // The timestamp has fixed width; only its date and hour bytes are needed.
        let field = 28 + find_byte(&line[28..], b' ').unwrap();
        let endpoint_start = field + 1;
        let endpoint_end = endpoint_start + find_byte(&line[endpoint_start..], b' ').unwrap();
        let status_start = endpoint_end + 1;
        let latency_start = status_start + 4;
        let endpoint_id = local.endpoint_id(&line[endpoint_start..endpoint_end]);
        let hour = hour_index(line);
        let slot = hour * local.ecap + endpoint_id;
        local.counts[slot] += 1;
        let is_error = u32::from(line[status_start] == b'5');
        local.errors[slot] += is_error;
        let latency = parse_uint(&line[latency_start..]);
        let key = hour * local.ecap + endpoint_id;
        // Store the dynamic-ECAP group key in the high word and latency in the low word.
        local.recs.push(((key as u64) << 32) | u64::from(latency));
        pos = if line_end < end { line_end + 1 } else { end };
    }
    local
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
        if self.len != 0 {
            // SAFETY: this is the unchanged address and length returned by mmap.
            unsafe {
                munmap(self.ptr.cast(), self.len);
            }
        }
    }
}

#[derive(Clone, Copy, Default)]
struct Stats {
    start: usize,
    end: usize,
    errors: u64,
    p50: u32,
    p95: u32,
    p99: u32,
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
    endpoints: &[&[u8]],
    groups: &[Stats],
    hours: &[[u8; 20]],
) -> Vec<u8> {
    let mut out = Vec::with_capacity(capacity);
    for hour in range {
        for (endpoint, &bytes) in endpoints.iter().enumerate() {
            let group = groups[hour * endpoints.len() + endpoint];
            if group.start == group.end {
                continue;
            }
            out.extend_from_slice(&hours[hour]);
            out.push(b' ');
            out.extend_from_slice(bytes);
            out.push(b' ');
            let count = (group.end - group.start) as u64;
            push_uint(&mut out, count);
            out.push(b' ');
            push_uint(&mut out, u64::from(group.p50));
            out.push(b' ');
            push_uint(&mut out, u64::from(group.p95));
            out.push(b' ');
            push_uint(&mut out, u64::from(group.p99));
            out.push(b' ');
            push_uint(&mut out, group.errors);
            out.push(b' ');
            push_uint(&mut out, group.errors * 1_000_000 / count);
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

    let parse_clock = std::time::Instant::now();
    let mut locals = thread::scope(|scope| {
        let joins = bounds
            .windows(2)
            .map(|pair| {
                let (start, end) = (pair[0], pair[1]);
                scope.spawn(move || parse_range(data, start, end))
            })
            .collect::<Vec<_>>();
        joins
            .into_iter()
            .map(|join| join.join().expect("parser thread panicked"))
            .collect::<Vec<_>>()
    });

    eprintln!("phase parse_ns={}", parse_clock.elapsed().as_nanos());
    let mut endpoints: Vec<&[u8]> = locals
        .iter()
        .flat_map(|local| local.endpoints.iter().copied())
        .collect();
    endpoints.sort_unstable();
    endpoints.dedup();
    let endpoint_count = endpoints.len();
    let group_count = HOURS * endpoint_count;
    let remaps: Vec<Vec<usize>> = locals
        .iter()
        .map(|local| {
            local
                .endpoints
                .iter()
                .map(|endpoint| endpoints.binary_search(endpoint).unwrap())
                .collect()
        })
        .collect();

    let mut totals = vec![0u64; group_count];
    for (local, remap) in locals.iter().zip(&remaps) {
        for hour in 0..HOURS {
            for (local_id, &global_id) in remap.iter().enumerate() {
                totals[hour * endpoint_count + global_id] +=
                    u64::from(local.counts[hour * local.ecap + local_id]);
            }
        }
    }
    let mut starts = vec![0usize; group_count + 1];
    for group in 0..group_count {
        starts[group + 1] = starts[group] + totals[group] as usize;
    }
    let record_count = starts[group_count];

    // Each worker receives the next disjoint range for every output group.
    let mut next = starts[..group_count].to_vec();
    let mut offsets = Vec::with_capacity(locals.len());
    for (local, remap) in locals.iter().zip(&remaps) {
        let mut worker_offsets = vec![0u32; group_count];
        for hour in 0..HOURS {
            for (local_id, &global_id) in remap.iter().enumerate() {
                let group = hour * endpoint_count + global_id;
                worker_offsets[group] = u32::try_from(next[group]).expect("too many records");
                next[group] += local.counts[hour * local.ecap + local_id] as usize;
            }
        }
        offsets.push(worker_offsets);
    }

    let mut latencies = vec![0u32; record_count];
    let latency_ptr = latencies.as_mut_ptr() as usize;
    thread::scope(|scope| {
        for ((local, mut worker_offsets), remap) in
            locals.iter_mut().zip(offsets).zip(remaps.iter())
        {
            let ptr = latency_ptr;
            scope.spawn(move || {
                let ecap = local.ecap;
                let ptr = ptr as *mut u32;
                for rec in &local.recs {
                    let key = (*rec >> 32) as usize;
                    let hour = key / ecap;
                    let local_id = key % ecap;
                    let group = hour * endpoint_count + remap[local_id];
                    let at = worker_offsets[group] as usize;
                    worker_offsets[group] += 1;
                    // SAFETY: offsets assign non-overlapping group slices to each worker; each
                    // record advances its own group's cursor exactly once within that slice.
                    unsafe { ptr.add(at).write(*rec as u32) };
                }
            });
        }
    });

    let mut errors = vec![0u64; group_count];
    for (local, remap) in locals.iter().zip(&remaps) {
        for hour in 0..HOURS {
            for (local_id, &global_id) in remap.iter().enumerate() {
                errors[hour * endpoint_count + global_id] +=
                    u64::from(local.errors[hour * local.ecap + local_id]);
            }
        }
    }
    drop(locals);

    let mut groups = vec![Stats::default(); group_count];
    for group in 0..group_count {
        groups[group] = Stats {
            start: starts[group],
            end: starts[group + 1],
            errors: errors[group],
            ..Stats::default()
        };
    }

    let total_work = record_count;
    let target_work = total_work.div_ceil(workers).max(1);
    let mut ranges = Vec::with_capacity(workers);
    let (mut first, mut work) = (0usize, 0usize);
    for (group, total) in totals.iter().enumerate() {
        work += *total as usize;
        if work >= target_work && ranges.len() + 1 < workers && group + 1 < group_count {
            ranges.push(first..group + 1);
            first = group + 1;
            work = 0;
        }
    }
    if first < group_count || ranges.is_empty() {
        ranges.push(first..group_count);
    }
    thread::scope(|scope| {
        let mut remaining_groups = groups.as_mut_slice();
        let mut remaining_latencies = latencies.as_mut_slice();
        for range in ranges {
            let group_len = range.len();
            let record_len = starts[range.end] - starts[range.start];
            let record_base = starts[range.start];
            let (group_slice, group_tail) = remaining_groups.split_at_mut(group_len);
            let (latency_slice, latency_tail) = remaining_latencies.split_at_mut(record_len);
            remaining_groups = group_tail;
            remaining_latencies = latency_tail;
            scope.spawn(move || {
                for group in group_slice.iter_mut() {
                    let n = group.end - group.start;
                    if n == 0 {
                        continue;
                    }
                    let start = group.start - record_base;
                    let values = &mut latency_slice[start..start + n];
                    values.sort_unstable();
                    group.p50 = values[(50 * n + 99) / 100 - 1];
                    group.p95 = values[(95 * n + 99) / 100 - 1];
                    group.p99 = values[(99 * n + 99) / 100 - 1];
                }
            });
        }
    });

    let hours = hour_strings();
    let mut hour_sizes = [0usize; HOURS];
    for hour in 0..HOURS {
        for endpoint in 0..endpoint_count {
            let group = groups[hour * endpoint_count + endpoint];
            if group.start != group.end {
                hour_sizes[hour] += endpoints[endpoint].len() + 48;
            }
        }
    }
    let target_size = hour_sizes.iter().sum::<usize>().div_ceil(workers).max(1);
    let mut hour_ranges = Vec::with_capacity(workers);
    let (mut start_hour, mut size) = (0usize, 0usize);
    for (hour, &weight) in hour_sizes.iter().enumerate() {
        size += weight;
        if size >= target_size && hour_ranges.len() + 1 < workers {
            hour_ranges.push((start_hour..hour + 1, size));
            start_hour = hour + 1;
            size = 0;
        }
    }
    if start_hour < HOURS || hour_ranges.is_empty() {
        hour_ranges.push((start_hour..HOURS, size));
    }
    let output_buffers = thread::scope(|scope| {
        let joins = hour_ranges
            .into_iter()
            .map(|(range, capacity)| {
                let (endpoints, groups, hours) = (&endpoints, &groups, &hours);
                scope.spawn(move || format_hours(range, capacity, endpoints, groups, hours))
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
        .read(true)
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
    use std::os::unix::{fs::FileExt, io::AsRawFd};
    unsafe extern "C" {
        fn mmap(
            addr: *mut std::ffi::c_void,
            length: usize,
            prot: i32,
            flags: i32,
            fd: i32,
            offset: isize,
        ) -> *mut std::ffi::c_void;
        fn munmap(addr: *mut std::ffi::c_void, length: usize) -> i32;
        fn _exit(status: i32) -> !;
    }
    if total_size != 0 {
        // SAFETY: the output is open read/write and was extended to the mapping length.
        let mapped = unsafe { mmap(std::ptr::null_mut(), total_size, 3, 1, file.as_raw_fd(), 0) };
        if mapped as isize != -1 {
            let address = mapped as usize;
            thread::scope(|scope| {
                for (buffer, offset) in output_buffers.into_iter().zip(offsets) {
                    scope.spawn(move || {
                        // SAFETY: prefix-summed output ranges are disjoint and in bounds.
                        unsafe {
                            std::ptr::copy_nonoverlapping(
                                buffer.as_ptr(),
                                (address as *mut u8).add(offset as usize),
                                buffer.len(),
                            )
                        };
                    });
                }
            });
            // SAFETY: these are the exact pointer and length returned by mmap.
            unsafe { munmap(mapped, total_size) };
            // SAFETY: all output writes have completed before this process exit.
            unsafe { _exit(0) }
        }
    }
    thread::scope(|scope| {
        for (buffer, offset) in output_buffers.into_iter().zip(offsets) {
            let file = &file;
            scope.spawn(move || {
                file.write_all_at(&buffer, offset)
                    .expect("write output range");
            });
        }
    });
    // SAFETY: all positioned writes have joined; process teardown closes the remaining handles.
    unsafe { _exit(0) }
}
