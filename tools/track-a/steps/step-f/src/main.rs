use std::cell::UnsafeCell;
use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Barrier, Mutex};
use std::thread::{self, ScopedJoinHandle};

const HOURS: usize = 72;
const MAX_THREADS: usize = 12;
const INITIAL_ECAP: usize = 2048;

struct JobSlot(UnsafeCell<Option<Box<dyn Fn(usize) + Send + Sync + 'static>>>);
// SAFETY: the main thread writes a job only while workers are parked at the barrier; workers
// read it only during the matching phase and finish before the main thread clears it.
unsafe impl Send for JobSlot {}
// SAFETY: access to the cell follows the barrier protocol documented above.
unsafe impl Sync for JobSlot {}

struct PersistentPool<'scope> {
    barrier: Arc<Barrier>,
    job: Arc<JobSlot>,
    stop: Arc<AtomicBool>,
    workers: Vec<ScopedJoinHandle<'scope, ()>>,
}

impl<'scope> PersistentPool<'scope> {
    fn new<'env: 'scope>(scope: &'scope thread::Scope<'scope, 'env>, count: usize) -> Self {
        let barrier = Arc::new(Barrier::new(count + 1));
        let job = Arc::new(JobSlot(UnsafeCell::new(None)));
        let stop = Arc::new(AtomicBool::new(false));
        let workers = (0..count)
            .map(|id| {
                let barrier = Arc::clone(&barrier);
                let job = Arc::clone(&job);
                let stop = Arc::clone(&stop);
                scope.spawn(move || loop {
                    barrier.wait();
                    if stop.load(Ordering::Acquire) {
                        break;
                    }
                    // SAFETY: the main thread publishes the job before the barrier and keeps
                    // it unchanged until every worker reaches the second barrier.
                    let task = unsafe { (&*job.0.get()).as_ref().expect("pool task") };
                    task(id);
                    barrier.wait();
                })
            })
            .collect();
        Self {
            barrier,
            job,
            stop,
            workers,
        }
    }

    fn run<F>(&self, task: F)
    where
        F: Fn(usize) + Send + Sync + 'static,
    {
        // SAFETY: all workers are parked after the previous phase; no worker reads this slot
        // until the following barrier releases them.
        unsafe {
            *self.job.0.get() = Some(Box::new(task));
        }
        self.barrier.wait();
        self.barrier.wait();
        // SAFETY: every worker has completed its task and reached the second barrier.
        unsafe {
            (*self.job.0.get()).take();
        }
    }

    fn stop(self) {
        self.stop.store(true, Ordering::Release);
        self.barrier.wait();
        for worker in self.workers {
            worker.join().expect("persistent worker panicked");
        }
    }
}

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

struct Local<'a> {
    endpoints: Vec<&'a [u8]>,
    endpoint_ids: FastMap<&'a [u8], usize>,
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
            endpoint_ids: FastMap::default(),
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
        if let Some(&id) = self.endpoint_ids.get(bytes) {
            return id;
        }
        let id = self.endpoints.len();
        if id == self.ecap {
            self.grow_ecap();
        }
        self.endpoint_ids.insert(bytes, id);
        self.endpoints.push(bytes);
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
    thread::scope(|scope| {
        let pool = PersistentPool::new(scope, workers);
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

        let data_ptr = data.as_ptr() as usize;
        let data_len = data.len();
        let local_slots: Arc<Vec<Mutex<Option<Local<'static>>>>> =
            Arc::new((0..workers).map(|_| Mutex::new(None)).collect());
        let worker_slots = Arc::clone(&local_slots);
        let bounds = Arc::new(bounds);
        pool.run(move |id| {
            // SAFETY: the mmap remains live until the pool has stopped and the process exits.
            let data = unsafe { std::slice::from_raw_parts(data_ptr as *const u8, data_len) };
            let local = parse_range(data, bounds[id], bounds[id + 1]);
            *worker_slots[id].lock().unwrap() = Some(local);
        });
        let locals: Vec<Local<'static>> = Arc::try_unwrap(local_slots)
            .ok()
            .unwrap()
            .into_iter()
            .map(|slot| slot.into_inner().unwrap().unwrap())
            .collect();

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
        for (local, remap) in locals.iter().zip(remaps.iter()) {
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
        let locals: Arc<Vec<Mutex<Local<'static>>>> =
            Arc::new(locals.into_iter().map(Mutex::new).collect());
        let offsets: Arc<Vec<Mutex<Vec<u32>>>> =
            Arc::new(offsets.into_iter().map(Mutex::new).collect());
        let remaps = Arc::new(remaps);
        let latency_ptr = latencies.as_mut_ptr() as usize;
        let worker_locals = Arc::clone(&locals);
        let worker_offsets = Arc::clone(&offsets);
        let worker_remaps = Arc::clone(&remaps);
        pool.run(move |worker| {
            let local = worker_locals[worker].lock().unwrap();
            let mut worker_offsets = worker_offsets[worker].lock().unwrap();
            let ptr = latency_ptr as *mut u32;
            let ecap = local.ecap;
            for rec in &local.recs {
                let key = (*rec >> 32) as usize;
                let hour = key / ecap;
                let local_id = key % ecap;
                let group = hour * endpoint_count + worker_remaps[worker][local_id];
                let at = worker_offsets[group] as usize;
                worker_offsets[group] += 1;
                // SAFETY: worker-specific offsets describe disjoint subranges of every group.
                unsafe { ptr.add(at).write(*rec as u32) };
            }
        });

        let mut errors = vec![0u64; group_count];
        for (local, remap) in locals.iter().zip(remaps.iter()) {
            let local = local.lock().unwrap();
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

        let group_ptr = groups.as_mut_ptr() as usize;
        let latency_ptr = latencies.as_mut_ptr() as usize;
        let next_group = Arc::new(AtomicUsize::new(0));
        let group_counter = Arc::clone(&next_group);
        pool.run(move |_| loop {
            let first = group_counter.fetch_add(256, Ordering::Relaxed);
            if first >= group_count {
                break;
            }
            let last = (first + 256).min(group_count);
            let groups = group_ptr as *mut Stats;
            let latencies = latency_ptr as *mut u32;
            for index in first..last {
                // SAFETY: each atomic batch owns distinct groups and therefore distinct latency slices.
                let group = unsafe { &mut *groups.add(index) };
                let n = group.end - group.start;
                if n == 0 {
                    continue;
                }
                // SAFETY: group starts/ends are the prefix-summed, in-bounds latency ranges.
                let values =
                    unsafe { std::slice::from_raw_parts_mut(latencies.add(group.start), n) };
                if n == 1 {
                    group.p50 = values[0];
                    group.p95 = values[0];
                    group.p99 = values[0];
                } else if n <= 48 {
                    values.sort_unstable();
                    group.p50 = values[(50 * n + 99) / 100 - 1];
                    group.p95 = values[(95 * n + 99) / 100 - 1];
                    group.p99 = values[(99 * n + 99) / 100 - 1];
                } else {
                    let i99 = (99 * n + 99) / 100 - 1;
                    let p99 = *values.select_nth_unstable(i99).1;
                    let i95 = (95 * n + 99) / 100 - 1;
                    let p95 = *values[..=i99].select_nth_unstable(i95).1;
                    let i50 = (50 * n + 99) / 100 - 1;
                    let p50 = *values[..=i95].select_nth_unstable(i50).1;
                    group.p50 = p50;
                    group.p95 = p95;
                    group.p99 = p99;
                }
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
        let tasks: Arc<Vec<_>> = Arc::new(
            (0..HOURS)
                .step_by(3)
                .map(|start| {
                    let end = (start + 3).min(HOURS);
                    (start..end, hour_sizes[start..end].iter().sum::<usize>())
                })
                .collect(),
        );
        let endpoints = Arc::new(endpoints);
        let groups = Arc::new(groups);
        let hours = Arc::new(hours);
        let output_slots: Arc<Vec<Mutex<Option<Vec<u8>>>>> =
            Arc::new((0..tasks.len()).map(|_| Mutex::new(None)).collect());
        let next_task = Arc::new(AtomicUsize::new(0));
        let worker_tasks = Arc::clone(&tasks);
        let worker_slots = Arc::clone(&output_slots);
        let worker_endpoints = Arc::clone(&endpoints);
        let worker_groups = Arc::clone(&groups);
        let worker_hours = Arc::clone(&hours);
        let task_counter = Arc::clone(&next_task);
        pool.run(move |_| loop {
            let task = task_counter.fetch_add(1, Ordering::Relaxed);
            if task >= worker_tasks.len() {
                break;
            }
            let (range, capacity) = worker_tasks[task].clone();
            let bytes = format_hours(
                range,
                capacity,
                &worker_endpoints,
                &worker_groups,
                &worker_hours,
            );
            *worker_slots[task].lock().unwrap() = Some(bytes);
        });
        let output_buffers: Vec<Vec<u8>> = Arc::try_unwrap(output_slots)
            .ok()
            .unwrap()
            .into_iter()
            .map(|slot| slot.into_inner().unwrap().unwrap())
            .collect();
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
        let output_buffers = Arc::new(output_buffers);
        let offsets = Arc::new(offsets);
        let file = Arc::new(file);
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
            let mapped =
                unsafe { mmap(std::ptr::null_mut(), total_size, 3, 1, file.as_raw_fd(), 0) };
            if mapped as isize != -1 {
                let address = mapped as usize;
                let next_output = Arc::new(AtomicUsize::new(0));
                let output_counter = Arc::clone(&next_output);
                let worker_buffers = Arc::clone(&output_buffers);
                let worker_offsets = Arc::clone(&offsets);
                pool.run(move |_| loop {
                    let index = output_counter.fetch_add(1, Ordering::Relaxed);
                    if index >= worker_buffers.len() {
                        break;
                    }
                    let buffer = &worker_buffers[index];
                    // SAFETY: each atomic task owns one prefix-summed disjoint output range.
                    unsafe {
                        std::ptr::copy_nonoverlapping(
                            buffer.as_ptr(),
                            (address as *mut u8).add(worker_offsets[index] as usize),
                            buffer.len(),
                        );
                    }
                });
                // SAFETY: these are the exact pointer and length returned by mmap.
                unsafe { munmap(mapped, total_size) };
                pool.stop();
                // SAFETY: all output writes have completed before this process exit.
                unsafe { _exit(0) }
            }
        }
        let next_output = Arc::new(AtomicUsize::new(0));
        let output_counter = Arc::clone(&next_output);
        let worker_buffers = Arc::clone(&output_buffers);
        let worker_offsets = Arc::clone(&offsets);
        let worker_file = Arc::clone(&file);
        pool.run(move |_| loop {
            let index = output_counter.fetch_add(1, Ordering::Relaxed);
            if index >= worker_buffers.len() {
                break;
            }
            worker_file
                .write_all_at(&worker_buffers[index], worker_offsets[index])
                .expect("write output range");
        });
        pool.stop();
        // SAFETY: all positioned writes have joined; process teardown closes remaining handles.
        unsafe { _exit(0) }
    });
}
