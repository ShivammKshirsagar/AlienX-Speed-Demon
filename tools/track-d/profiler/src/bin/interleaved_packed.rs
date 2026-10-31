use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;

const INF: u64 = u64::MAX;
const EMPTY_SLOT: u32 = u32::MAX;
const INF32: u32 = u32::MAX;
const INIT_CAP: usize = 16384;
const SHRINK_THRESH: usize = 262144;
const MAX_CAP: usize = 524288;

unsafe extern "C" {
    fn mmap(a: *mut u8, l: usize, p: i32, f: i32, d: i32, o: isize) -> *mut u8;
    fn munmap(a: *mut u8, l: usize) -> i32;
    fn mallopt(param: i32, value: i32) -> i32;
    fn malloc_trim(pad: usize) -> i32;
}

struct Mapping {
    ptr: *mut u8,
    len: usize,
}

unsafe impl Send for Mapping {}
unsafe impl Sync for Mapping {}

impl Mapping {
    fn open(path: &Path) -> Self {
        use std::os::unix::io::AsRawFd;
        let file = std::fs::File::open(path).expect("open input");
        let len = file.metadata().expect("input metadata").len() as usize;
        if len == 0 {
            return Self {
                ptr: std::ptr::NonNull::<u8>::dangling().as_ptr(),
                len: 0,
            };
        }
        let ptr = unsafe { mmap(std::ptr::null_mut(), len, 1, 2, file.as_raw_fd(), 0) };
        assert_ne!(ptr as isize, -1, "mmap failed");
        Self { ptr, len }
    }

    #[inline(always)]
    fn bytes(&self) -> &[u8] {
        if self.len == 0 {
            &[]
        } else {
            unsafe { std::slice::from_raw_parts(self.ptr, self.len) }
        }
    }
}

impl Drop for Mapping {
    fn drop(&mut self) {
        if self.len != 0 {
            unsafe {
                munmap(self.ptr, self.len);
            }
        }
    }
}

#[inline(always)]
unsafe fn parse_u32_raw(ptr: *const u8, pos: &mut usize) -> u32 {
    let mut n = (*ptr.add(*pos) - b'0') as u32;
    *pos += 1;
    while *ptr.add(*pos) >= b'0' && *ptr.add(*pos) <= b'9' {
        n = n * 10 + (*ptr.add(*pos) - b'0') as u32;
        *pos += 1;
    }
    n
}

#[derive(Copy, Clone)]
#[repr(C)]
struct Edge(u32, u32);

#[derive(Copy, Clone)]
struct Slot(u32, u32);

struct CompactDist {
    slots: Vec<Slot>,
    mask: usize,
    touched: Vec<usize>,
}

impl CompactDist {
    fn new() -> Self {
        Self {
            slots: vec![Slot(EMPTY_SLOT, INF32); INIT_CAP],
            mask: INIT_CAP - 1,
            touched: Vec::with_capacity(1024),
        }
    }

    #[inline(always)]
    fn get(&self, node: u32) -> u32 {
        let mut i = (node as usize).wrapping_mul(0x9E3779B9) & self.mask;
        loop {
            let s = unsafe { self.slots.get_unchecked(i) };
            if s.0 == node {
                return s.1;
            }
            if s.0 == EMPTY_SLOT {
                return INF32;
            }
            i = (i + 1) & self.mask;
        }
    }

    fn grow(&mut self) -> bool {
        let new_cap = self.slots.len() * 2;
        if new_cap > MAX_CAP {
            return false;
        }
        let new_mask = new_cap - 1;
        let mut new_slots = vec![Slot(EMPTY_SLOT, INF32); new_cap];
        let mut new_touched = Vec::with_capacity(self.touched.len() * 2);
        for &idx in &self.touched {
            let slot = self.slots[idx];
            let mut nidx = (slot.0 as usize).wrapping_mul(0x9E3779B9) & new_mask;
            while new_slots[nidx].0 != EMPTY_SLOT {
                nidx = (nidx + 1) & new_mask;
            }
            new_slots[nidx] = slot;
            new_touched.push(nidx);
        }
        self.slots = new_slots;
        self.mask = new_mask;
        self.touched = new_touched;
        true
    }

    #[inline(always)]
    fn try_relax(&mut self, node: u32, new_dist: u32) -> Option<bool> {
        if self.touched.len() * 2 >= self.mask && !self.grow() {
            return None;
        }
        let mut i = (node as usize).wrapping_mul(0x9E3779B9) & self.mask;
        loop {
            let s = unsafe { self.slots.get_unchecked_mut(i) };
            if s.0 == node {
                if new_dist < s.1 {
                    s.1 = new_dist;
                    return Some(true);
                }
                return Some(false);
            }
            if s.0 == EMPTY_SLOT {
                *s = Slot(node, new_dist);
                self.touched.push(i);
                return Some(true);
            }
            i = (i + 1) & self.mask;
        }
    }

    #[inline(always)]
    fn clear_and_shrink(&mut self) {
        for &idx in &self.touched {
            unsafe {
                *self.slots.get_unchecked_mut(idx) = Slot(EMPTY_SLOT, INF32);
            }
        }
        self.touched.clear();
        if self.slots.len() >= SHRINK_THRESH {
            self.slots = vec![Slot(EMPTY_SLOT, INF32); INIT_CAP];
            self.mask = INIT_CAP - 1;
            self.touched.shrink_to(1024);
        }
    }
}

#[derive(Copy, Clone)]
struct HeapItem(u64, u32);

struct QuadHeap(Vec<HeapItem>);

impl QuadHeap {
    fn new() -> Self {
        Self(Vec::with_capacity(1024))
    }

    #[inline(always)]
    fn push(&mut self, dist: u64, node: u32) {
        let mut i = self.0.len();
        self.0.push(HeapItem(dist, node));
        while i > 0 {
            let p = (i - 1) >> 2;
            if self.0[i].0 < self.0[p].0 {
                self.0.swap(i, p);
                i = p;
            } else {
                break;
            }
        }
    }

    #[inline(always)]
    fn pop(&mut self) -> Option<(u64, u32)> {
        if self.0.is_empty() {
            return None;
        }
        let res = self.0.swap_remove(0);
        let len = self.0.len();
        let mut i = 0;
        while (i << 2) + 1 < len {
            let b = (i << 2) + 1;
            let mut m = b;
            for c in (b + 1)..(b + 4).min(len) {
                if self.0[c].0 < self.0[m].0 {
                    m = c;
                }
            }
            if self.0[m].0 < self.0[i].0 {
                self.0.swap(i, m);
                i = m;
            } else {
                break;
            }
        }
        Some((res.0, res.1))
    }

    #[inline(always)]
    fn clear_and_shrink(&mut self) {
        self.0.clear();
        if self.0.capacity() > 4096 {
            self.0.shrink_to(1024);
        }
    }
}

static FALLBACK_LOCK: AtomicBool = AtomicBool::new(false);

fn fallback_dijkstra_standard(
    s: u32,
    t: u32,
    n: u32,
    offsets: &[u32],
    edges: &[Edge],
    heap: &mut QuadHeap,
) -> u64 {
    while FALLBACK_LOCK.swap(true, Ordering::Acquire) {
        std::hint::spin_loop();
    }
    let mut dist = vec![INF; n as usize];
    heap.clear_and_shrink();
    dist[s as usize] = 0;
    heap.push(0, s);
    let mut ans = INF;
    while let Some((d, u)) = heap.pop() {
        if d > dist[u as usize] {
            continue;
        }
        if u == t {
            ans = d;
            break;
        }
        let start = offsets[u as usize] as usize;
        let end = offsets[u as usize + 1] as usize;
        for i in start..end {
            let e = unsafe { edges.get_unchecked(i) };
            let nd = d + e.1 as u64;
            if nd < dist[e.0 as usize] {
                dist[e.0 as usize] = nd;
                heap.push(nd, e.0);
            }
        }
    }
    heap.clear_and_shrink();
    drop(dist);
    unsafe {
        malloc_trim(0);
    }
    FALLBACK_LOCK.store(false, Ordering::Release);
    ans
}

fn fallback_dijkstra_packed(
    s: u32,
    t: u32,
    n: u32,
    offsets: &[u32],
    packed_edges: &[u32],
    w_bits: u32,
    w_mask: u32,
    escape_w: u32,
    heap: &mut QuadHeap,
) -> u64 {
    while FALLBACK_LOCK.swap(true, Ordering::Acquire) {
        std::hint::spin_loop();
    }
    let mut dist = vec![INF; n as usize];
    heap.clear_and_shrink();
    dist[s as usize] = 0;
    heap.push(0, s);
    let mut ans = INF;
    while let Some((d, u)) = heap.pop() {
        if d > dist[u as usize] {
            continue;
        }
        if u == t {
            ans = d;
            break;
        }
        let start = offsets[u as usize] as usize;
        let end = offsets[u as usize + 1] as usize;
        let mut i = start;
        while i < end {
            let packed = unsafe { *packed_edges.get_unchecked(i) };
            let v = packed >> w_bits;
            let w_field = packed & w_mask;
            let (w, step) = if w_field != escape_w {
                (w_field as u64, 1)
            } else {
                let real_w = unsafe { *packed_edges.get_unchecked(i + 1) as u64 };
                (real_w, 2)
            };
            i += step;
            let nd = d + w;
            if nd < dist[v as usize] {
                dist[v as usize] = nd;
                heap.push(nd, v);
            }
        }
    }
    heap.clear_and_shrink();
    drop(dist);
    unsafe {
        malloc_trim(0);
    }
    FALLBACK_LOCK.store(false, Ordering::Release);
    ans
}

// Background sampler for anon memory
fn read_anon_kb() -> Option<u64> {
    if let Ok(content) = std::fs::read_to_string("/proc/self/smaps_rollup") {
        for line in content.lines() {
            if line.starts_with("Anonymous:") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 {
                    if let Ok(kb) = parts[1].parse::<u64>() {
                        return Some(kb);
                    }
                }
            }
        }
    }
    None
}

enum StepResult {
    Continue,
    Done(u64),
    NeedFallback,
}

struct SearchState {
    compact: CompactDist,
    heap: QuadHeap,
    s: u32,
    t: u32,
    query_idx: usize,
    active: bool,
}

impl SearchState {
    fn new() -> Self {
        Self {
            compact: CompactDist::new(),
            heap: QuadHeap::new(),
            s: 0,
            t: 0,
            query_idx: 0,
            active: false,
        }
    }

    #[inline(always)]
    fn init(&mut self, s: u32, t: u32, query_idx: usize) {
        self.s = s;
        self.t = t;
        self.query_idx = query_idx;
        self.active = true;
        self.compact.try_relax(s, 0);
        self.heap.push(0, s);
    }

    #[inline(always)]
    fn clear(&mut self) {
        self.compact.clear_and_shrink();
        self.heap.clear_and_shrink();
    }

    #[inline(always)]
    fn step_standard(
        &mut self,
        offsets: &[u32],
        edges: &[Edge],
        local_settled: &mut u64,
    ) -> StepResult {
        loop {
            let (d, u) = match self.heap.pop() {
                Some(item) => item,
                None => return StepResult::Done(INF),
            };
            if let Some(&HeapItem(_, next_u)) = self.heap.0.first() {
                let next_off = unsafe { *offsets.get_unchecked(next_u as usize) as usize };
                unsafe {
                    std::arch::x86_64::_mm_prefetch(
                        edges.as_ptr().add(next_off) as *const i8,
                        std::arch::x86_64::_MM_HINT_T0,
                    );
                }
            }
            let cur = self.compact.get(u);
            if cur == INF32 || d > cur as u64 {
                continue;
            }
            *local_settled += 1;
            if u == self.t {
                return StepResult::Done(d);
            }
            let start = unsafe { *offsets.get_unchecked(u as usize) as usize };
            let end = unsafe { *offsets.get_unchecked(u as usize + 1) as usize };
            for i in start..end {
                let e = unsafe { edges.get_unchecked(i) };
                let nd = d + e.1 as u64;
                if nd >= INF32 as u64 {
                    return StepResult::NeedFallback;
                }
                match self.compact.try_relax(e.0, nd as u32) {
                    Some(true) => {
                        self.heap.push(nd, e.0);
                        unsafe {
                            std::arch::x86_64::_mm_prefetch(
                                offsets.as_ptr().add(e.0 as usize) as *const i8,
                                std::arch::x86_64::_MM_HINT_T0,
                            );
                        }
                    }
                    Some(false) => {}
                    None => return StepResult::NeedFallback,
                }
            }
            return StepResult::Continue;
        }
    }

    #[inline(always)]
    fn step_packed(
        &mut self,
        offsets: &[u32],
        packed_edges: &[u32],
        w_bits: u32,
        w_mask: u32,
        escape_w: u32,
        local_settled: &mut u64,
    ) -> StepResult {
        loop {
            let (d, u) = match self.heap.pop() {
                Some(item) => item,
                None => return StepResult::Done(INF),
            };
            if let Some(&HeapItem(_, next_u)) = self.heap.0.first() {
                let next_off = unsafe { *offsets.get_unchecked(next_u as usize) as usize };
                unsafe {
                    std::arch::x86_64::_mm_prefetch(
                        packed_edges.as_ptr().add(next_off) as *const i8,
                        std::arch::x86_64::_MM_HINT_T0,
                    );
                }
            }
            let cur = self.compact.get(u);
            if cur == INF32 || d > cur as u64 {
                continue;
            }
            *local_settled += 1;
            if u == self.t {
                return StepResult::Done(d);
            }
            let start = unsafe { *offsets.get_unchecked(u as usize) as usize };
            let end = unsafe { *offsets.get_unchecked(u as usize + 1) as usize };
            let mut i = start;
            while i < end {
                let packed = unsafe { *packed_edges.get_unchecked(i) };
                let v = packed >> w_bits;
                let w_field = packed & w_mask;
                let (w, step) = if w_field != escape_w {
                    (w_field as u64, 1)
                } else {
                    let real_w = unsafe { *packed_edges.get_unchecked(i + 1) as u64 };
                    (real_w, 2)
                };
                i += step;
                let nd = d + w;
                if nd >= INF32 as u64 {
                    return StepResult::NeedFallback;
                }
                match self.compact.try_relax(v, nd as u32) {
                    Some(true) => {
                        self.heap.push(nd, v);
                        unsafe {
                            std::arch::x86_64::_mm_prefetch(
                                offsets.as_ptr().add(v as usize) as *const i8,
                                std::arch::x86_64::_MM_HINT_T0,
                            );
                        }
                    }
                    Some(false) => {}
                    None => return StepResult::NeedFallback,
                }
            }
            return StepResult::Continue;
        }
    }
}

fn main() {
    unsafe {
        mallopt(-1, 65536); // M_TRIM_THRESHOLD
        mallopt(-3, 65536); // M_MMAP_THRESHOLD
        mallopt(-8, 2); // M_ARENA_MAX
    }

    let args: Vec<String> = std::env::args().collect();
    if args.len() < 4 {
        eprintln!(
            "Usage: {} <graph.txt> <queries.txt> <out.txt> [interleave_N: 1,2,4,8] [packed: 0/1] [workers]",
            args[0]
        );
        std::process::exit(1);
    }
    let graph_path = Path::new(&args[1]);
    let queries_path = Path::new(&args[2]);
    let out_path = Path::new(&args[3]);

    let interleave_n: usize = args.get(4).and_then(|s| s.parse().ok()).unwrap_or(1);
    let use_packed: bool = args
        .get(5)
        .and_then(|s| s.parse::<u32>().ok())
        .map(|v| v != 0)
        .unwrap_or(false);
    let workers: usize = args
        .get(6)
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(|| {
            std::env::var("WORKERS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(12)
        });

    // Start background peak anon memory sampler
    let peak_anon_kb = Arc::new(AtomicU64::new(read_anon_kb().unwrap_or(0)));
    let sampler_stop = Arc::new(AtomicBool::new(false));
    let sampler_handle = {
        let peak_anon = peak_anon_kb.clone();
        let stop = sampler_stop.clone();
        thread::spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                if let Some(kb) = read_anon_kb() {
                    peak_anon.fetch_max(kb, Ordering::Relaxed);
                }
                thread::sleep(std::time::Duration::from_millis(5));
            }
        })
    };

    let t_total_start = std::time::Instant::now();

    // 1. Parse Graph Header
    let t_build_start = std::time::Instant::now();
    let graph_map = Mapping::open(graph_path);
    let g_data = graph_map.bytes();
    let g_ptr = g_data.as_ptr();
    let mut g_pos = 0;
    let n = unsafe { parse_u32_raw(g_ptr, &mut g_pos) };
    g_pos += 1;
    let m = unsafe { parse_u32_raw(g_ptr, &mut g_pos) };
    while g_pos < g_data.len() && (g_data[g_pos] == b'\r' || g_data[g_pos] == b'\n') {
        g_pos += 1;
    }
    let edges_data = &g_data[g_pos..];

    let num_threads = workers.min(m.max(1) as usize);

    let mut bounds = Vec::with_capacity(num_threads + 1);
    bounds.push(0);
    let chunk_len = edges_data.len() / num_threads;
    for i in 1..num_threads {
        let mut target = i * chunk_len;
        while target < edges_data.len() && edges_data[target] != b'\n' {
            target += 1;
        }
        if target < edges_data.len() {
            target += 1;
        }
        bounds.push(target);
    }
    bounds.push(edges_data.len());

    // Packing configuration if use_packed is true
    let target_bits = 32 - (n.saturating_sub(1)).leading_zeros();
    let w_bits = 32 - target_bits;
    let w_mask = (1u32 << w_bits) - 1;
    let escape_w = w_mask;

    let degrees: Vec<AtomicU32> = (0..n).map(|_| AtomicU32::new(0)).collect();

    thread::scope(|scope| {
        for i in 0..num_threads {
            let start = bounds[i];
            let end = bounds[i + 1];
            let degrees = &degrees;
            scope.spawn(move || {
                let slice = &edges_data[start..end];
                let ptr = slice.as_ptr();
                let mut cur = 0;
                while cur < slice.len() {
                    if slice[cur] == b'\n' || slice[cur] == b'\r' {
                        cur += 1;
                        continue;
                    }
                    let u = unsafe { parse_u32_raw(ptr, &mut cur) };
                    if use_packed {
                        cur += 1;
                        // skip v
                        while cur < slice.len() && slice[cur] != b' ' && slice[cur] != b'\t' {
                            cur += 1;
                        }
                        cur += 1;
                        let w = unsafe { parse_u32_raw(ptr, &mut cur) };
                        while cur < slice.len() && slice[cur] != b'\n' {
                            cur += 1;
                        }
                        if cur < slice.len() {
                            cur += 1;
                        }
                        let words = if w < escape_w { 1 } else { 2 };
                        let deg = unsafe { degrees.get_unchecked(u as usize) };
                        deg.fetch_add(words, Ordering::Relaxed);
                    } else {
                        let deg = unsafe { degrees.get_unchecked(u as usize) };
                        deg.fetch_add(1, Ordering::Relaxed);
                        while cur < slice.len() && slice[cur] != b'\n' {
                            cur += 1;
                        }
                        if cur < slice.len() {
                            cur += 1;
                        }
                    }
                }
            });
        }
    });

    let mut offsets = vec![0u32; n as usize + 1];
    let mut sum = 0u32;
    for u in 0..n as usize {
        offsets[u] = sum;
        sum += degrees[u].load(Ordering::Relaxed);
    }
    offsets[n as usize] = sum;
    drop(degrees);

    let write_pos = (0..n)
        .map(|u| AtomicU32::new(offsets[u as usize]))
        .collect::<Vec<_>>();

    // Standard edges OR packed edges
    let mut std_edges = if !use_packed {
        vec![Edge(0, 0); sum as usize]
    } else {
        Vec::new()
    };
    let mut packed_edges = if use_packed {
        vec![0u32; sum as usize]
    } else {
        Vec::new()
    };

    let std_edges_ptr = std_edges.as_mut_ptr() as usize;
    let packed_edges_ptr = packed_edges.as_mut_ptr() as usize;

    thread::scope(|scope| {
        for i in 0..num_threads {
            let start = bounds[i];
            let end = bounds[i + 1];
            let write_pos = &write_pos;
            scope.spawn(move || {
                let slice = &edges_data[start..end];
                let ptr = slice.as_ptr();
                let mut cur = 0;
                let se_ptr = std_edges_ptr as *mut Edge;
                let pe_ptr = packed_edges_ptr as *mut u32;

                while cur < slice.len() {
                    if slice[cur] == b'\n' || slice[cur] == b'\r' {
                        cur += 1;
                        continue;
                    }
                    let u = unsafe { parse_u32_raw(ptr, &mut cur) };
                    cur += 1;
                    let v = unsafe { parse_u32_raw(ptr, &mut cur) };
                    cur += 1;
                    let w = unsafe { parse_u32_raw(ptr, &mut cur) };
                    while cur < slice.len() && (slice[cur] == b'\r' || slice[cur] == b'\n') {
                        cur += 1;
                    }
                    let wp = unsafe { write_pos.get_unchecked(u as usize) };
                    if use_packed {
                        if w < escape_w {
                            let p = wp.fetch_add(1, Ordering::Relaxed) as usize;
                            let packed = (v << w_bits) | w;
                            unsafe {
                                *pe_ptr.add(p) = packed;
                            }
                        } else {
                            let p = wp.fetch_add(2, Ordering::Relaxed) as usize;
                            let packed = (v << w_bits) | escape_w;
                            unsafe {
                                *pe_ptr.add(p) = packed;
                                *pe_ptr.add(p + 1) = w;
                            }
                        }
                    } else {
                        let p = wp.fetch_add(1, Ordering::Relaxed) as usize;
                        unsafe {
                            *se_ptr.add(p) = Edge(v, w);
                        }
                    }
                }
            });
        }
    });
    drop(write_pos);
    drop(graph_map);
    unsafe {
        malloc_trim(0);
    }

    let mut has_in = vec![false; n as usize];
    if use_packed {
        let mut i = 0;
        let tot = packed_edges.len();
        while i < tot {
            let packed = packed_edges[i];
            let v = (packed >> w_bits) as usize;
            let w_field = packed & w_mask;
            if v < n as usize {
                has_in[v] = true;
            }
            if w_field == escape_w {
                i += 2;
            } else {
                i += 1;
            }
        }
    } else {
        for e in &std_edges {
            has_in[e.0 as usize] = true;
        }
    }

    let t_build_ms = t_build_start.elapsed().as_secs_f64() * 1000.0;

    // Load Queries
    let queries_map = Mapping::open(queries_path);
    let q_data = queries_map.bytes();
    let q_ptr = q_data.as_ptr();
    let mut queries = Vec::new();
    let mut q_pos = 0;
    while q_pos < q_data.len() {
        while q_pos < q_data.len() && (q_data[q_pos] == b'\n' || q_data[q_pos] == b'\r') {
            q_pos += 1;
        }
        if q_pos >= q_data.len() {
            break;
        }
        let s = unsafe { parse_u32_raw(q_ptr, &mut q_pos) };
        q_pos += 1;
        let t = unsafe { parse_u32_raw(q_ptr, &mut q_pos) };
        while q_pos < q_data.len() && (q_data[q_pos] == b'\r' || q_data[q_pos] == b'\n') {
            q_pos += 1;
        }
        queries.push((s, t));
    }
    drop(queries_map);

    let num_queries = queries.len();
    let results: Vec<AtomicU64> = (0..num_queries).map(|_| AtomicU64::new(INF)).collect();
    let mut to_solve = Vec::with_capacity(num_queries);
    for (idx, &(s, t)) in queries.iter().enumerate() {
        if s == t {
            results[idx].store(0, Ordering::Relaxed);
        } else if offsets[s as usize + 1] != offsets[s as usize] && has_in[t as usize] {
            to_solve.push(idx);
        }
    }

    // Query Phase
    let t_query_start = std::time::Instant::now();
    let total_settled = AtomicU64::new(0);

    if !to_solve.is_empty() {
        let work_idx = AtomicUsize::new(0);
        let q_workers = workers.min(to_solve.len());

        thread::scope(|scope| {
            for _ in 0..q_workers {
                let work_idx = &work_idx;
                let to_solve = &to_solve;
                let queries = &queries;
                let results = &results;
                let offsets = &offsets;
                let std_edges = &std_edges;
                let packed_edges = &packed_edges;
                let total_settled = &total_settled;

                scope.spawn(move || {
                    let mut states: Vec<SearchState> =
                        (0..interleave_n).map(|_| SearchState::new()).collect();
                    let mut local_settled = 0u64;

                    // Initialize states
                    for state in states.iter_mut() {
                        let widx = work_idx.fetch_add(1, Ordering::Relaxed);
                        if widx < to_solve.len() {
                            let idx = to_solve[widx];
                            let (s, t) = queries[idx];
                            state.init(s, t, idx);
                        }
                    }

                    let mut active_count = states.iter().filter(|s| s.active).count();
                    let mut cursor = 0;

                    while active_count > 0 {
                        let state = &mut states[cursor];
                        if state.active {
                            let step_res = if use_packed {
                                state.step_packed(
                                    offsets,
                                    packed_edges,
                                    w_bits,
                                    w_mask,
                                    escape_w,
                                    &mut local_settled,
                                )
                            } else {
                                state.step_standard(offsets, std_edges, &mut local_settled)
                            };

                            match step_res {
                                StepResult::Continue => {}
                                StepResult::Done(ans) => {
                                    results[state.query_idx].store(ans, Ordering::Relaxed);
                                    state.clear();
                                    let widx = work_idx.fetch_add(1, Ordering::Relaxed);
                                    if widx < to_solve.len() {
                                        let idx = to_solve[widx];
                                        let (s, t) = queries[idx];
                                        state.init(s, t, idx);
                                    } else {
                                        state.active = false;
                                        active_count -= 1;
                                    }
                                }
                                StepResult::NeedFallback => {
                                    let ans = if use_packed {
                                        fallback_dijkstra_packed(
                                            state.s,
                                            state.t,
                                            n,
                                            offsets,
                                            packed_edges,
                                            w_bits,
                                            w_mask,
                                            escape_w,
                                            &mut state.heap,
                                        )
                                    } else {
                                        fallback_dijkstra_standard(
                                            state.s,
                                            state.t,
                                            n,
                                            offsets,
                                            std_edges,
                                            &mut state.heap,
                                        )
                                    };
                                    results[state.query_idx].store(ans, Ordering::Relaxed);
                                    state.clear();
                                    let widx = work_idx.fetch_add(1, Ordering::Relaxed);
                                    if widx < to_solve.len() {
                                        let idx = to_solve[widx];
                                        let (s, t) = queries[idx];
                                        state.init(s, t, idx);
                                    } else {
                                        state.active = false;
                                        active_count -= 1;
                                    }
                                }
                            }
                        }
                        cursor = (cursor + 1) % interleave_n;
                    }
                    total_settled.fetch_add(local_settled, Ordering::Relaxed);
                });
            }
        });
    }

    let query_ms = t_query_start.elapsed().as_secs_f64() * 1000.0;
    let total_ms = t_total_start.elapsed().as_secs_f64() * 1000.0;
    let settled = total_settled.load(Ordering::Relaxed);

    // Stop background memory sampler
    sampler_stop.store(true, Ordering::Relaxed);
    let _ = sampler_handle.join();
    let peak_anon_mb = (peak_anon_kb.load(Ordering::Relaxed) as f64) / 1024.0;

    // Format & write output
    let mut out_buf = Vec::with_capacity(num_queries * 12);
    for i in 0..num_queries {
        let ans = results[i].load(Ordering::Relaxed);
        if ans == INF {
            out_buf.extend_from_slice(b"-1\n");
        } else {
            let mut d = [b'\n'; 21];
            let mut p = 20;
            let mut v = ans;
            loop {
                p -= 1;
                d[p] = b'0' + (v % 10) as u8;
                v /= 10;
                if v == 0 {
                    break;
                }
            }
            out_buf.extend_from_slice(&d[p..]);
        }
    }
    if let Some(parent) = Path::new(&out_path).parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    std::fs::write(out_path, &out_buf).expect("write output");

    let ns_per_node = if settled > 0 {
        (query_ms * 1e6) / (settled as f64)
    } else {
        0.0
    };

    println!(
        "RESULT: N={} packed={} workers={} build={:.2}ms query={:.2}ms total={:.2}ms peak_anon={:.2}MB settled={} ns_per_node={:.1}",
        interleave_n, if use_packed { 1 } else { 0 }, workers, t_build_ms, query_ms, total_ms, peak_anon_mb, settled, ns_per_node
    );
}
