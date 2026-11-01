use std::cell::UnsafeCell;
use std::fs::{File, OpenOptions};
use std::os::unix::fs::FileExt;
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU8, AtomicUsize, Ordering};
use std::sync::Barrier;
use std::thread;

const HOURS: usize = 72;
const WORKERS: usize = 12;
const ECAP: usize = 256;
const TWO_PASS: bool = true;
const LAST_CACHE: bool = false;
const PARSE: u8 = 1;
const PREFIX: u8 = 2;
const PREFIX_ADD: u8 = 3;
const SCATTER: u8 = 4;
const FILL: u8 = 5;
const SELECT: u8 = 6;
const OUTPUT: u8 = 7;

struct Shared<T>(UnsafeCell<T>);
// SAFETY: Shared values are changed only between barrier phases. During a phase, workers
// mutate distinct cells or disjoint slices; the phase-specific rules are documented below.
unsafe impl<T: Send> Sync for Shared<T> {}

#[derive(Clone, Copy, Default)]
struct Slot {
    hash: u64,
    key_offset: u32,
    len: u32,
    id: u32,
    used: bool,
}

struct EndpointTable<'a> {
    slots: Vec<Slot>,
    keys: Vec<&'a [u8]>,
}

impl<'a> EndpointTable<'a> {
    fn new() -> Self {
        Self {
            slots: vec![Slot::default(); 256],
            keys: Vec::new(),
        }
    }

    fn hash(key: &[u8]) -> u64 {
        let mut h = (key.len() as u64) ^ 0x9e3779b97f4a7c15;
        let mut at = 0;
        while at + 8 <= key.len() {
            // SAFETY: the loop condition guarantees eight readable bytes at this offset.
            let word = unsafe { std::ptr::read_unaligned(key.as_ptr().add(at).cast::<u64>()) };
            h = (h ^ word).wrapping_mul(0xbf58476d1ce4e5b9);
            at += 8;
        }
        for &byte in &key[at..] {
            h = (h ^ u64::from(byte)).wrapping_mul(0x94d049bb133111eb);
        }
        (h ^ (h >> 30)).wrapping_mul(0xbf58476d1ce4e5b9) ^ (h >> 27)
    }

    fn find(&self, key: &[u8]) -> Option<u32> {
        let hash = Self::hash(key);
        let mask = self.slots.len() - 1;
        let mut at = hash as usize & mask;
        loop {
            let slot = self.slots[at];
            if !slot.used {
                return None;
            }
            if slot.hash == hash
                && slot.len as usize == key.len()
                && self.keys[slot.key_offset as usize] == key
            {
                return Some(slot.id);
            }
            at = (at + 1) & mask;
        }
    }

    fn place(&mut self, key_offset: u32, id: u32) {
        let key = self.keys[key_offset as usize];
        let slot = Slot {
            hash: Self::hash(key),
            key_offset,
            len: key.len() as u32,
            id,
            used: true,
        };
        let mask = self.slots.len() - 1;
        let mut at = slot.hash as usize & mask;
        while self.slots[at].used {
            at = (at + 1) & mask;
        }
        self.slots[at] = slot;
    }

    fn intern(&mut self, key: &'a [u8]) -> (u32, bool) {
        if let Some(id) = self.find(key) {
            return (id, false);
        }
        if (self.keys.len() + 1) * 10 >= self.slots.len() * 7 {
            let new_len = self.slots.len() * 2;
            let old = std::mem::replace(&mut self.slots, vec![Slot::default(); new_len]);
            for slot in old.into_iter().filter(|s| s.used) {
                let key = self.keys[slot.key_offset as usize];
                let mask = self.slots.len() - 1;
                let mut at = slot.hash as usize & mask;
                while self.slots[at].used {
                    at = (at + 1) & mask;
                }
                self.slots[at] = Slot {
                    len: key.len() as u32,
                    ..slot
                };
            }
        }
        let id = u32::try_from(self.keys.len()).expect("endpoint id exceeds u32");
        let offset = id;
        self.keys.push(key);
        self.place(offset, id);
        (id, true)
    }
}

#[derive(Clone, Copy, Default)]
struct Group {
    start: u32,
    count: u32,
    errors: u32,
    p50: u32,
    p95: u32,
    p99: u32,
}

struct Worker<'a> {
    table: EndpointTable<'a>,
    counts: Vec<u32>,
    errors: Vec<u32>,
    ecap: usize,
    remap: Vec<u32>,
    records: Vec<u64>,
    fill_cursor: Vec<u32>,
    output: Vec<u8>,
    last_key: &'a [u8],
    last_id: u32,
    has_last: bool,
}

impl<'a> Worker<'a> {
    fn new() -> Self {
        Self {
            table: EndpointTable::new(),
            counts: vec![0; HOURS * ECAP],
            errors: vec![0; HOURS * ECAP],
            ecap: ECAP,
            remap: Vec::new(),
            records: Vec::new(),
            fill_cursor: Vec::new(),
            output: Vec::new(),
            last_key: &[],
            last_id: 0,
            has_last: false,
        }
    }

    fn grow(&mut self, needed: usize) {
        if needed < self.ecap {
            return;
        }
        let old_cap = self.ecap;
        let new_cap = old_cap
            .checked_mul(2)
            .expect("ECAP overflow")
            .max(needed + 1);
        let mut counts = vec![0; HOURS * new_cap];
        let mut errors = vec![0; HOURS * new_cap];
        for hour in 0..HOURS {
            counts[hour * new_cap..hour * new_cap + old_cap]
                .copy_from_slice(&self.counts[hour * old_cap..(hour + 1) * old_cap]);
            errors[hour * new_cap..hour * new_cap + old_cap]
                .copy_from_slice(&self.errors[hour * old_cap..(hour + 1) * old_cap]);
        }
        self.counts = counts;
        self.errors = errors;
        self.ecap = new_cap;
    }

    fn endpoint(&mut self, key: &'a [u8]) -> u32 {
        if LAST_CACHE && self.has_last && self.last_key == key {
            return self.last_id;
        }
        let (id, inserted) = self.table.intern(key);
        if inserted {
            self.grow(id as usize);
        }
        if LAST_CACHE {
            self.last_key = key;
            self.last_id = id;
            self.has_last = true;
        }
        id
    }
}

struct Mapping {
    ptr: *mut u8,
    len: usize,
}

// SAFETY: the mapping is read-only and remains alive until all scoped workers finish.
unsafe impl Sync for Mapping {}

impl Mapping {
    fn open(path: &Path) -> Self {
        let file = File::open(path).expect("open input");
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
        // SAFETY: the read-only mapping is retained for the program lifetime and MAP_FAILED is checked.
        let ptr = unsafe { mmap(std::ptr::null_mut(), len, 1, 2, file.as_raw_fd(), 0) };
        assert_ne!(ptr as isize, -1, "mmap input");
        Self {
            ptr: ptr.cast(),
            len,
        }
    }

    fn bytes(&self) -> &[u8] {
        // SAFETY: mmap returned a readable region of exactly len bytes, retained by this value.
        unsafe { std::slice::from_raw_parts(self.ptr, self.len) }
    }
}

impl Drop for Mapping {
    fn drop(&mut self) {
        unsafe extern "C" {
            fn munmap(addr: *mut std::ffi::c_void, length: usize) -> i32;
        }
        if self.len != 0 {
            // SAFETY: these are the unchanged pointer and length returned by mmap.
            unsafe {
                munmap(self.ptr.cast(), self.len);
            }
        }
    }
}

#[derive(Clone, Copy)]
struct Chunk {
    start: usize,
    end: usize,
}

fn chunks(data: &[u8]) -> Vec<Chunk> {
    if data.is_empty() {
        return Vec::new();
    }
    let n = 48.min(data.len());
    let mut bounds = Vec::with_capacity(n + 1);
    bounds.push(0);
    for i in 1..n {
        let mut p = data.len() * i / n;
        while p < data.len() && data[p - 1] != b'\n' {
            p += 1;
        }
        if p > *bounds.last().unwrap() && p < data.len() {
            bounds.push(p);
        }
    }
    if *bounds.last().unwrap() < data.len() {
        bounds.push(data.len());
    }
    bounds
        .windows(2)
        .map(|w| Chunk {
            start: w[0],
            end: w[1],
        })
        .collect()
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn line_fields(line: &[u8]) -> (usize, usize, usize) {
    use std::arch::x86_64::*;
    let (mut p, mut first, mut second, mut found) = (28, 0, 0, 0);
    while p + 32 <= line.len() {
        // SAFETY: p+32 <= line.len() guarantees a full readable vector.
        let x = unsafe { _mm256_loadu_si256(line.as_ptr().add(p).cast()) };
        let spaces =
            _mm256_movemask_epi8(_mm256_cmpeq_epi8(x, _mm256_set1_epi8(b' ' as i8))) as u32;
        let newlines =
            _mm256_movemask_epi8(_mm256_cmpeq_epi8(x, _mm256_set1_epi8(b'\n' as i8))) as u32;
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
        while mask != 0 && found < 2 {
            let at = p + mask.trailing_zeros() as usize;
            if found == 0 {
                first = at;
            } else {
                second = at;
            }
            found += 1;
            mask &= mask - 1;
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

fn hour_index(ts: &[u8]) -> usize {
    let month = usize::from((ts[5] - b'0') * 10 + ts[6] - b'0');
    let day = usize::from((ts[8] - b'0') * 10 + ts[9] - b'0');
    let hour = usize::from((ts[11] - b'0') * 10 + ts[12] - b'0');
    ((month - 10) * 31 + day - 30) * 24 + hour - 18
}

fn parse_uint(bytes: &[u8]) -> u32 {
    let mut value = 0u32;
    let mut at = 0;
    if bytes.len() >= 8 {
        // SAFETY: the length check guarantees eight readable ASCII digits.
        let word = u64::from_le(unsafe { std::ptr::read_unaligned(bytes.as_ptr().cast()) });
        for shift in 0..8 {
            value = value * 10 + ((word >> (shift * 8)) as u8 - b'0') as u32;
        }
        at = 8;
    }
    for &byte in &bytes[at..] {
        value = value * 10 + u32::from(byte - b'0');
    }
    value
}

unsafe extern "C" {
    fn getpagesize() -> i32;
    fn madvise(addr: *mut std::ffi::c_void, len: usize, advice: i32) -> i32;
}

fn populate_chunk(mapping: &Mapping, chunk: Chunk) {
    if chunk.end == chunk.start {
        return;
    }
    // SAFETY: getpagesize has no preconditions and is called within this Linux-only program.
    let page = unsafe { getpagesize() as usize };
    if page == 0 {
        return;
    }
    let start = chunk.start / page * page;
    let end = chunk.end.div_ceil(page) * page;
    let end = end.min(mapping.len);
    // SAFETY: the aligned subrange lies inside the live mmap; MADV_POPULATE_READ errors are ignored.
    unsafe {
        madvise(mapping.ptr.add(start).cast(), end - start, 22);
    }
}

fn parse_chunk<'a>(worker: &mut Worker<'a>, data: &'a [u8], chunk: Chunk, store: bool) {
    let mut pos = chunk.start;
    while pos < chunk.end {
        // SAFETY: the scanner guards vector loads and the chunk ends on a line boundary.
        let (line_len, first, second) = unsafe { line_fields(&data[pos..chunk.end]) };
        let line_end = pos + line_len;
        let line = &data[pos..line_end];
        let endpoint = &line[first + 1..second];
        let hour = hour_index(line);
        let id = worker.endpoint(endpoint) as usize;
        let cell = hour * worker.ecap + id;
        worker.counts[cell] += 1;
        if line[second + 1] == b'5' {
            worker.errors[cell] += 1;
        }
        if store {
            let key = u32::try_from(id * HOURS + hour).expect("local group key exceeds u32");
            worker
                .records
                .push((u64::from(key) << 32) | u64::from(parse_uint(&line[second + 5..])));
        }
        pos = if line_end < chunk.end {
            line_end + 1
        } else {
            chunk.end
        };
    }
}

fn parse_fill(worker: &mut Worker<'_>, data: &[u8], chunk: Chunk, latencies: *mut u32) {
    let mut pos = chunk.start;
    while pos < chunk.end {
        // SAFETY: the scanner guards vector loads and the chunk ends on a line boundary.
        let (line_len, first, second) = unsafe { line_fields(&data[pos..chunk.end]) };
        let line_end = pos + line_len;
        let line = &data[pos..line_end];
        let hour = hour_index(line);
        let endpoint = &line[first + 1..second];
        let id = worker
            .table
            .find(endpoint)
            .expect("endpoint missing on second pass") as usize;
        let cell = hour * worker.ecap + id;
        let at = worker.fill_cursor[cell] as usize;
        worker.fill_cursor[cell] += 1;
        // SAFETY: each worker's cursor owns a disjoint interval for each global group.
        unsafe {
            latencies.add(at).write(parse_uint(&line[second + 5..]));
        }
        pos = if line_end < chunk.end {
            line_end + 1
        } else {
            chunk.end
        };
    }
}

fn phase(phase: &AtomicU8, barrier: &Barrier, value: u8) {
    phase.store(value, Ordering::Release);
    barrier.wait();
    barrier.wait();
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

fn digit_count(mut n: u64) -> usize {
    let mut digits = 1;
    while n >= 10 {
        n /= 10;
        digits += 1;
    }
    digits
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
            out.copy_from_slice(format!("2026-{month:02}-{date:02}T{hour:02}:00:00Z").as_bytes());
            out
        })
        .collect()
}

fn format_hour(
    out: &mut Vec<u8>,
    hour: usize,
    ne: usize,
    endpoints: &[&[u8]],
    groups: &[Group],
    hours: &[[u8; 20]],
) {
    for rank in 0..ne {
        let group = groups[hour * ne + rank];
        if group.count == 0 {
            continue;
        }
        out.extend_from_slice(&hours[hour]);
        out.push(b' ');
        out.extend_from_slice(endpoints[rank]);
        out.push(b' ');
        push_uint(out, u64::from(group.count));
        out.push(b' ');
        push_uint(out, u64::from(group.p50));
        out.push(b' ');
        push_uint(out, u64::from(group.p95));
        out.push(b' ');
        push_uint(out, u64::from(group.p99));
        out.push(b' ');
        push_uint(out, u64::from(group.errors));
        out.push(b' ');
        push_uint(
            out,
            u64::from(group.errors) * 1_000_000 / u64::from(group.count),
        );
        out.push(b'\n');
    }
}

fn output_line_len(endpoint_len: usize, group: Group) -> usize {
    20 + 1
        + endpoint_len
        + 1
        + digit_count(u64::from(group.count))
        + 1
        + digit_count(u64::from(group.p50))
        + 1
        + digit_count(u64::from(group.p95))
        + 1
        + digit_count(u64::from(group.p99))
        + 1
        + digit_count(u64::from(group.errors))
        + 1
        + digit_count(u64::from(group.errors) * 1_000_000 / u64::from(group.count))
        + 1
}

fn worker_loop<'a>(
    id: usize,
    states: &[Shared<Worker<'a>>],
    data: &'a [u8],
    mapping: &Mapping,
    chunks: &[Chunk],
    owners: &[AtomicUsize],
    endpoints: &Shared<Vec<&'a [u8]>>,
    table: &Shared<EndpointTable<'a>>,
    groups: &Shared<Vec<Group>>,
    latencies: &Shared<Vec<u32>>,
    two_pass_offsets: &Shared<Vec<u32>>,
    partials: &Shared<Vec<u64>>,
    bases: &Shared<Vec<u64>>,
    order: &Shared<Vec<usize>>,
    output_offsets: &Shared<Vec<u64>>,
    file: &File,
    phase: &AtomicU8,
    barrier: &Barrier,
    next_chunk: &AtomicUsize,
    next_task: &AtomicUsize,
    next_group: &AtomicUsize,
    next_hour: &AtomicUsize,
) {
    loop {
        barrier.wait();
        match phase.load(Ordering::Acquire) {
            0 => break,
            PARSE => {
                // SAFETY: each worker mutates only the state cell indexed by its unique id.
                let worker = unsafe { &mut *states[id].0.get() };
                let first = next_chunk.fetch_add(1, Ordering::Relaxed);
                if first < chunks.len() {
                    let chunk = chunks[first];
                    owners[first].store(id, Ordering::Relaxed);
                    if worker.records.capacity() == 0 {
                        worker
                            .records
                            .reserve(chunk.end.saturating_sub(chunk.start) / 40);
                    }
                    populate_chunk(mapping, chunk);
                    parse_chunk(worker, data, chunk, !TWO_PASS);
                }
                loop {
                    let task = next_chunk.fetch_add(1, Ordering::Relaxed);
                    if task >= chunks.len() {
                        break;
                    }
                    owners[task].store(id, Ordering::Relaxed);
                    parse_chunk(worker, data, chunks[task], !TWO_PASS);
                }
            }
            PREFIX => {
                let group_count = unsafe { (&*groups.0.get()).len() };
                let group_ptr = unsafe { (&*groups.0.get()).as_ptr() as *mut Group };
                let partial_ptr = unsafe { (&*partials.0.get()).as_ptr() as *mut u64 };
                loop {
                    let block = next_task.fetch_add(1, Ordering::Relaxed);
                    if block >= unsafe { (&*partials.0.get()).len() } {
                        break;
                    }
                    let start = block * 1024;
                    let end = (start + 1024).min(group_count);
                    let mut sum = 0u64;
                    for g in start..end {
                        // SAFETY: this block is uniquely claimed and group ranges do not overlap.
                        let mut item = unsafe { *group_ptr.add(g) };
                        item.start = u32::try_from(sum).expect("prefix exceeds u32");
                        sum += u64::from(item.count);
                        // SAFETY: this block is uniquely claimed and group ranges do not overlap.
                        unsafe {
                            group_ptr.add(g).write(item);
                        }
                    }
                    // SAFETY: each claimed block writes its own partial sum.
                    unsafe {
                        partial_ptr.add(block).write(sum);
                    }
                }
            }
            PREFIX_ADD => {
                let group_count = unsafe { (&*groups.0.get()).len() };
                let group_ptr = unsafe { (&*groups.0.get()).as_ptr() as *mut Group };
                let base_ptr = unsafe { (&*bases.0.get()).as_ptr() };
                let block_count = unsafe { (&*bases.0.get()).len() };
                loop {
                    let block = next_task.fetch_add(1, Ordering::Relaxed);
                    if block >= block_count {
                        break;
                    }
                    let start = block * 1024;
                    let end = (start + 1024).min(group_count);
                    let base = unsafe { *base_ptr.add(block) };
                    for g in start..end {
                        // SAFETY: each group belongs to exactly one uniquely claimed block.
                        let item = unsafe { &mut *group_ptr.add(g) };
                        item.start = u32::try_from(u64::from(item.start) + base)
                            .expect("prefix exceeds u32");
                    }
                }
            }
            SCATTER => {
                let latency_ptr = unsafe { (&*latencies.0.get()).as_ptr() as *mut u32 };
                loop {
                    let state_id = next_task.fetch_add(1, Ordering::Relaxed);
                    if state_id >= states.len() {
                        break;
                    }
                    // SAFETY: each worker claims a unique state id; main is parked at the barrier.
                    let state = unsafe { &mut *states[state_id].0.get() };
                    for &record in &state.records {
                        let local_group = (record >> 32) as usize;
                        let id = local_group / HOURS;
                        let hour = local_group % HOURS;
                        let cell = hour * state.ecap + id;
                        let at = state.counts[cell] as usize;
                        state.counts[cell] += 1;
                        // SAFETY: prefix offsets assign each worker a disjoint interval per group.
                        unsafe {
                            latency_ptr.add(at).write(record as u32);
                        }
                    }
                    state.records = Vec::new();
                    state.counts = Vec::new();
                    state.errors = Vec::new();
                    state.remap = Vec::new();
                }
            }
            FILL => {
                let latency_ptr = unsafe { (&*latencies.0.get()).as_ptr() as *mut u32 };
                // SAFETY: this worker exclusively mutates its own state during FILL.
                let worker = unsafe { &mut *states[id].0.get() };
                for (task, chunk) in chunks.iter().enumerate() {
                    if owners[task].load(Ordering::Relaxed) == id {
                        parse_fill(worker, data, *chunk, latency_ptr);
                    }
                }
            }
            SELECT => {
                let group_ptr = unsafe { (&*groups.0.get()).as_ptr() as *mut Group };
                let latency_ptr = unsafe { (&*latencies.0.get()).as_ptr() as *mut u32 };
                let order = unsafe { &*order.0.get() };
                loop {
                    let task = next_group.fetch_add(1, Ordering::Relaxed);
                    if task >= order.len() {
                        break;
                    }
                    let g = order[task];
                    // SAFETY: each group appears once in order and is claimed once atomically.
                    let mut group = unsafe { *group_ptr.add(g) };
                    let start = group.start as usize;
                    let n = group.count as usize;
                    if n == 1 {
                        let value = unsafe { *latency_ptr.add(start) };
                        group.p50 = value;
                        group.p95 = value;
                        group.p99 = value;
                    } else if n <= 48 {
                        // SAFETY: each group has a disjoint range established by the prefix sum.
                        let values =
                            unsafe { std::slice::from_raw_parts_mut(latency_ptr.add(start), n) };
                        values.sort_unstable();
                        group.p50 = values[(50 * n + 99) / 100 - 1];
                        group.p95 = values[(95 * n + 99) / 100 - 1];
                        group.p99 = values[(99 * n + 99) / 100 - 1];
                    } else {
                        // SAFETY: each group owns a disjoint latency interval.
                        let values =
                            unsafe { std::slice::from_raw_parts_mut(latency_ptr.add(start), n) };
                        let r99 = (99 * n + 99) / 100 - 1;
                        let (left99, p99, _) = values.select_nth_unstable(r99);
                        group.p99 = *p99;
                        let r95 = (95 * n + 99) / 100 - 1;
                        let (left95, p95, _) = left99.select_nth_unstable(r95);
                        group.p95 = *p95;
                        let r50 = (50 * n + 99) / 100 - 1;
                        group.p50 = left95.select_nth_unstable(r50).1.to_owned();
                    }
                    // SAFETY: this group is uniquely claimed; no other worker touches its fields.
                    unsafe {
                        group_ptr.add(g).write(group);
                    }
                }
            }
            OUTPUT => {
                let endpoints = unsafe { &*endpoints.0.get() };
                let groups = unsafe { &*groups.0.get() };
                let ne = endpoints.len();
                let hours = hour_strings();
                // SAFETY: this output buffer belongs exclusively to the current worker id.
                let state = unsafe { &mut *states[id].0.get() };
                loop {
                    let hour = next_hour.fetch_add(1, Ordering::Relaxed);
                    if hour >= HOURS {
                        break;
                    }
                    state.output.clear();
                    format_hour(&mut state.output, hour, ne, endpoints, groups, &hours);
                    let offsets = unsafe { &*output_offsets.0.get() };
                    file.write_all_at(&state.output, offsets[hour])
                        .expect("write output hour");
                }
            }
            _ => unreachable!(),
        }
        barrier.wait();
    }
}

fn main() {
    let mut args = std::env::args_os().skip(1);
    let input: PathBuf = args
        .next()
        .map_or_else(|| "/data/requests.log".into(), Into::into);
    let output: PathBuf = args
        .next()
        .map_or_else(|| "/out/summary.txt".into(), Into::into);
    let mapping = Mapping::open(Path::new(&input));
    let data = mapping.bytes();
    let chunk_list = chunks(data);
    let workers = thread::available_parallelism()
        .map_or(1, usize::from)
        .min(WORKERS)
        .max(1);
    let states: Vec<Shared<Worker>> = (0..workers)
        .map(|_| Shared(UnsafeCell::new(Worker::new())))
        .collect();
    let endpoints = Shared(UnsafeCell::new(Vec::<&[u8]>::new()));
    let table = Shared(UnsafeCell::new(EndpointTable::new()));
    let groups = Shared(UnsafeCell::new(Vec::<Group>::new()));
    let latencies = Shared(UnsafeCell::new(Vec::<u32>::new()));
    let two_pass_offsets = Shared(UnsafeCell::new(Vec::<u32>::new()));
    let partials = Shared(UnsafeCell::new(Vec::<u64>::new()));
    let bases = Shared(UnsafeCell::new(Vec::<u64>::new()));
    let order = Shared(UnsafeCell::new(Vec::<usize>::new()));
    let output_offsets = Shared(UnsafeCell::new(Vec::<u64>::new()));
    let phase_code = AtomicU8::new(0);
    let barrier = Barrier::new(workers + 1);
    let next_chunk = AtomicUsize::new(0);
    let owners: Vec<AtomicUsize> = (0..chunk_list.len())
        .map(|_| AtomicUsize::new(usize::MAX))
        .collect();
    let next_task = AtomicUsize::new(0);
    let next_group = AtomicUsize::new(0);
    let next_hour = AtomicUsize::new(0);
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent).expect("create output directory");
    }
    let file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&output)
        .expect("open output");
    unsafe extern "C" {
        fn _exit(status: i32) -> !;
    }

    thread::scope(|scope| {
        for id in 0..workers {
            let states = &states;
            let data = data;
            let mapping = &mapping;
            let chunks = &chunk_list;
            let owners = &owners;
            let endpoints = &endpoints;
            let table = &table;
            let groups = &groups;
            let latencies = &latencies;
            let two_pass_offsets = &two_pass_offsets;
            let partials = &partials;
            let bases = &bases;
            let order = &order;
            let output_offsets = &output_offsets;
            let file = &file;
            let phase_code = &phase_code;
            let barrier = &barrier;
            let next_chunk = &next_chunk;
            let next_task = &next_task;
            let next_group = &next_group;
            let next_hour = &next_hour;
            scope.spawn(move || {
                worker_loop(
                    id,
                    states,
                    data,
                    mapping,
                    chunks,
                    owners,
                    endpoints,
                    table,
                    groups,
                    latencies,
                    two_pass_offsets,
                    partials,
                    bases,
                    order,
                    output_offsets,
                    file,
                    phase_code,
                    barrier,
                    next_chunk,
                    next_task,
                    next_group,
                    next_hour,
                )
            });
        }

        // The parsing workers need only the immutable input and their own local state.
        next_chunk.store(0, Ordering::Relaxed);
        phase(&phase_code, &barrier, PARSE);

        let mut global_endpoints: Vec<&[u8]> = Vec::new();
        for cell in &states {
            // SAFETY: workers are parked at the phase barrier; main exclusively reads local tables.
            let worker = unsafe { &*cell.0.get() };
            global_endpoints.extend(worker.table.keys.iter().copied());
        }
        global_endpoints.sort_unstable();
        global_endpoints.dedup();
        let ne = global_endpoints.len();
        // SAFETY: workers are parked while the immutable global endpoint list is installed.
        unsafe {
            *endpoints.0.get() = global_endpoints.clone();
        }
        let group_count = HOURS.checked_mul(ne).expect("group count overflow");
        let group_ptr = unsafe { &mut *groups.0.get() };
        *group_ptr = vec![Group::default(); group_count];
        let mut remaps = Vec::with_capacity(workers);
        for cell in &states {
            // SAFETY: workers are parked; main exclusively updates this worker's remap.
            let worker = unsafe { &mut *cell.0.get() };
            worker.remap = worker
                .table
                .keys
                .iter()
                .map(|key| global_endpoints.binary_search(key).unwrap() as u32)
                .collect();
            remaps.push(worker.remap.clone());
        }
        for (worker_id, cell) in states.iter().enumerate() {
            // SAFETY: workers are parked; this loop is the sole access to worker counts/errors.
            let worker = unsafe { &mut *cell.0.get() };
            for (id, &rank) in remaps[worker_id].iter().enumerate() {
                for hour in 0..HOURS {
                    let local = hour * worker.ecap + id;
                    let group = hour * ne + rank as usize;
                    group_ptr[group].count = group_ptr[group]
                        .count
                        .checked_add(worker.counts[local])
                        .expect("group count overflow");
                    group_ptr[group].errors = group_ptr[group]
                        .errors
                        .checked_add(worker.errors[local])
                        .expect("group errors overflow");
                }
            }
        }

        const PREFIX_BLOCK: usize = 1024;
        let block_count = group_count.div_ceil(PREFIX_BLOCK);
        unsafe {
            *partials.0.get() = vec![0; block_count];
            *bases.0.get() = vec![0; block_count];
        }
        next_task.store(0, Ordering::Relaxed);
        phase(&phase_code, &barrier, PREFIX);
        let mut total = 0u64;
        // SAFETY: workers are parked; the main thread computes each block's exclusive base.
        let sums = unsafe { &*partials.0.get() };
        let bases_mut = unsafe { &mut *bases.0.get() };
        for (i, &sum) in sums.iter().enumerate() {
            bases_mut[i] = total;
            total += sum;
        }
        assert!(
            total <= u64::from(u32::MAX),
            "too many records for u32 offsets"
        );
        next_task.store(0, Ordering::Relaxed);
        phase(&phase_code, &barrier, PREFIX_ADD);

        if TWO_PASS {
            let mut cursors: Vec<u32> =
                unsafe { (&*groups.0.get()).iter().map(|g| g.start).collect() };
            for cell in &states {
                // SAFETY: workers are parked; main reads this worker's counts and remap.
                let worker = unsafe { &mut *cell.0.get() };
                worker.fill_cursor = worker.counts.clone();
                for (id, &rank) in worker.remap.iter().enumerate() {
                    for hour in 0..HOURS {
                        let local = hour * worker.ecap + id;
                        let g = hour * ne + rank as usize;
                        let count = worker.counts[local];
                        worker.fill_cursor[local] = cursors[g];
                        cursors[g] += count;
                    }
                }
                worker.errors = Vec::new();
            }
        } else {
            let mut cursors: Vec<u32> =
                unsafe { (&*groups.0.get()).iter().map(|g| g.start).collect() };
            for cell in &states {
                // SAFETY: workers are parked; main converts counts into disjoint write cursors.
                let worker = unsafe { &mut *cell.0.get() };
                for (id, &rank) in worker.remap.iter().enumerate() {
                    for hour in 0..HOURS {
                        let local = hour * worker.ecap + id;
                        let g = hour * ne + rank as usize;
                        let count = worker.counts[local];
                        worker.counts[local] = cursors[g];
                        cursors[g] += count;
                    }
                }
                worker.errors = Vec::new();
                worker.table = EndpointTable::new();
            }
        }
        unsafe {
            *latencies.0.get() = vec![0u32; total as usize];
        }

        if TWO_PASS {
            next_chunk.store(0, Ordering::Relaxed);
            phase(&phase_code, &barrier, FILL);
            for cell in &states {
                // SAFETY: workers are parked after FILL; local scan state is no longer used.
                let worker = unsafe { &mut *cell.0.get() };
                worker.fill_cursor = Vec::new();
                worker.counts = Vec::new();
                worker.remap = Vec::new();
                worker.table = EndpointTable::new();
            }
        } else {
            next_task.store(0, Ordering::Relaxed);
            phase(&phase_code, &barrier, SCATTER);
        }

        let mut selection_order: Vec<usize> = unsafe {
            (&*groups.0.get())
                .iter()
                .enumerate()
                .filter_map(|(i, g)| (g.count != 0).then_some(i))
                .collect()
        };
        selection_order
            .sort_unstable_by_key(|&i| std::cmp::Reverse(unsafe { (&*groups.0.get())[i].count }));
        unsafe {
            *order.0.get() = selection_order;
        }
        next_group.store(0, Ordering::Relaxed);
        phase(&phase_code, &barrier, SELECT);

        let group_ref = unsafe { &*groups.0.get() };
        let endpoints_ref = global_endpoints.as_slice();
        let hours = hour_strings();
        let mut hour_sizes = vec![0u64; HOURS];
        for hour in 0..HOURS {
            for rank in 0..ne {
                let group = group_ref[hour * ne + rank];
                if group.count != 0 {
                    hour_sizes[hour] += output_line_len(endpoints_ref[rank].len(), group) as u64;
                }
            }
        }
        let mut offsets = vec![0u64; HOURS];
        let mut output_len = 0u64;
        for hour in 0..HOURS {
            offsets[hour] = output_len;
            output_len += hour_sizes[hour];
        }
        file.set_len(output_len).expect("size output");
        // SAFETY: workers are parked and read the offsets only in the following OUTPUT phase.
        unsafe {
            *output_offsets.0.get() = offsets;
        }
        let _ = (&hours, endpoints_ref);
        next_hour.store(0, Ordering::Relaxed);
        phase(&phase_code, &barrier, OUTPUT);
        // SAFETY: every output task completed and all worker writes joined at the barrier.
        unsafe extern "C" {
            fn _exit(status: i32) -> !;
        }
        // SAFETY: output is complete; process teardown must skip all destructors and mmap drop.
        unsafe { _exit(0) }
    });
}
