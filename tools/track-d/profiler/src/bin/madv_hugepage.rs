use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::thread;

const INF: u64 = u64::MAX;
const EMPTY_SLOT: u32 = u32::MAX;
const INF32: u32 = u32::MAX;
const INIT_CAP: usize = 16384;
const SHRINK_THRESH: usize = 262144;
const MAX_CAP: usize = 524288;
const HUGEPAGE_2MB: usize = 2 * 1024 * 1024;
const MADV_HUGEPAGE: i32 = 14;

unsafe extern "C" {
    fn mmap(a: *mut u8, l: usize, p: i32, f: i32, d: i32, o: isize) -> *mut u8;
    fn munmap(a: *mut u8, l: usize) -> i32;
    fn mallopt(param: i32, value: i32) -> i32;
    fn malloc_trim(pad: usize) -> i32;
    fn madvise(addr: *mut u8, len: usize, advice: i32) -> i32;
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
            return Self { ptr: std::ptr::NonNull::<u8>::dangling().as_ptr(), len: 0 };
        }
        let ptr = unsafe { mmap(std::ptr::null_mut(), len, 1, 2, file.as_raw_fd(), 0) };
        assert_ne!(ptr as isize, -1, "mmap failed");
        Self { ptr, len }
    }

    #[inline(always)]
    fn bytes(&self) -> &[u8] {
        if self.len == 0 { &[] } else {
            unsafe { std::slice::from_raw_parts(self.ptr, self.len) }
        }
    }
}

impl Drop for Mapping {
    fn drop(&mut self) {
        if self.len != 0 {
            unsafe { munmap(self.ptr, self.len); }
        }
    }
}

// 2 MB aligned buffer for MADV_HUGEPAGE
struct HugeBuf<T> {
    ptr: *mut T,
    len: usize,
    alloc_bytes: usize,
    use_huge: bool,
}

impl<T: Copy> HugeBuf<T> {
    fn with_capacity_zeroed(len: usize, use_huge: bool) -> Self {
        use std::alloc::{alloc_zeroed, Layout};
        let elem_bytes = std::mem::size_of::<T>();
        let total_bytes = len * elem_bytes;
        let alloc_bytes = if use_huge {
            (total_bytes + HUGEPAGE_2MB - 1) & !(HUGEPAGE_2MB - 1)
        } else {
            total_bytes
        };
        let align = if use_huge { HUGEPAGE_2MB } else { std::mem::align_of::<T>() };
        let layout = Layout::from_size_align(alloc_bytes, align).expect("valid layout");
        let ptr = unsafe { alloc_zeroed(layout) as *mut T };
        assert!(!ptr.is_null(), "allocation failed");

        if use_huge {
            let ret = unsafe { madvise(ptr as *mut u8, alloc_bytes, MADV_HUGEPAGE) };
            if ret != 0 {
                eprintln!("madvise(MADV_HUGEPAGE) failed with code {}", ret);
            }
        }
        Self { ptr, len, alloc_bytes, use_huge }
    }
}

impl<T> std::ops::Deref for HugeBuf<T> {
    type Target = [T];
    #[inline(always)]
    fn deref(&self) -> &[T] {
        unsafe { std::slice::from_raw_parts(self.ptr, self.len) }
    }
}

impl<T> std::ops::DerefMut for HugeBuf<T> {
    #[inline(always)]
    fn deref_mut(&mut self) -> &mut [T] {
        unsafe { std::slice::from_raw_parts_mut(self.ptr, self.len) }
    }
}

impl<T> Drop for HugeBuf<T> {
    fn drop(&mut self) {
        use std::alloc::{dealloc, Layout};
        let align = if self.use_huge { HUGEPAGE_2MB } else { std::mem::align_of::<T>() };
        let layout = Layout::from_size_align(self.alloc_bytes, align).unwrap();
        unsafe { dealloc(self.ptr as *mut u8, layout); }
    }
}

unsafe impl<T: Send> Send for HugeBuf<T> {}
unsafe impl<T: Sync> Sync for HugeBuf<T> {}

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
            if s.0 == node { return s.1; }
            if s.0 == EMPTY_SLOT { return INF32; }
            i = (i + 1) & self.mask;
        }
    }

    fn grow(&mut self) -> bool {
        let new_cap = self.slots.len() * 2;
        if new_cap > MAX_CAP { return false; }
        let new_mask = new_cap - 1;
        let mut new_slots = vec![Slot(EMPTY_SLOT, INF32); new_cap];
        let mut new_touched = Vec::with_capacity(self.touched.len() * 2);
        for &idx in &self.touched {
            let slot = self.slots[idx];
            let mut nidx = (slot.0 as usize).wrapping_mul(0x9E3779B9) & new_mask;
            while new_slots[nidx].0 != EMPTY_SLOT { nidx = (nidx + 1) & new_mask; }
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
        if self.touched.len() * 2 >= self.mask && !self.grow() { return None; }
        let mut i = (node as usize).wrapping_mul(0x9E3779B9) & self.mask;
        loop {
            let s = unsafe { self.slots.get_unchecked_mut(i) };
            if s.0 == node {
                if new_dist < s.1 { s.1 = new_dist; return Some(true); }
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
            unsafe { *self.slots.get_unchecked_mut(idx) = Slot(EMPTY_SLOT, INF32); }
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
    fn new() -> Self { Self(Vec::with_capacity(1024)) }

    #[inline(always)]
    fn push(&mut self, dist: u64, node: u32) {
        let mut i = self.0.len();
        self.0.push(HeapItem(dist, node));
        while i > 0 {
            let p = (i - 1) >> 2;
            if self.0[i].0 < self.0[p].0 {
                self.0.swap(i, p);
                i = p;
            } else { break; }
        }
    }

    #[inline(always)]
    fn pop(&mut self) -> Option<(u64, u32)> {
        if self.0.is_empty() { return None; }
        let res = self.0.swap_remove(0);
        let len = self.0.len();
        let mut i = 0;
        while (i << 2) + 1 < len {
            let b = (i << 2) + 1;
            let mut m = b;
            for c in (b + 1)..(b + 4).min(len) {
                if self.0[c].0 < self.0[m].0 { m = c; }
            }
            if self.0[m].0 < self.0[i].0 {
                self.0.swap(i, m);
                i = m;
            } else { break; }
        }
        Some((res.0, res.1))
    }

    #[inline(always)]
    fn clear_and_shrink(&mut self) {
        self.0.clear();
        if self.0.capacity() > 4096 { self.0.shrink_to(1024); }
    }
}

static FALLBACK_LOCK: AtomicBool = AtomicBool::new(false);

fn fallback_dijkstra(
    s: u32, t: u32, n: u32,
    offsets: &[u32], edges: &[Edge],
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
        if d > dist[u as usize] { continue; }
        if u == t { ans = d; break; }
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
    unsafe { malloc_trim(0); }
    FALLBACK_LOCK.store(false, Ordering::Release);
    ans
}

fn run_pipeline(
    graph_path: &Path, queries_path: &Path,
    use_huge: bool, workers: usize, count_settled: bool
) -> (f64, f64, f64, u64) {
    let t_total = std::time::Instant::now();

    let g_map = Mapping::open(graph_path);
    let g_bytes = g_map.bytes();
    let mut g_pos = 0;
    let n = unsafe { parse_u32_raw(g_bytes.as_ptr(), &mut g_pos) };
    while g_bytes[g_pos] == b' ' || g_bytes[g_pos] == b'\t' { g_pos += 1; }
    let m = unsafe { parse_u32_raw(g_bytes.as_ptr(), &mut g_pos) };
    while g_bytes[g_pos] != b'\n' { g_pos += 1; }
    g_pos += 1;

    let degrees: Vec<AtomicU32> = (0..n).map(|_| AtomicU32::new(0)).collect();
    let num_threads = workers.min(m.max(1) as usize);
    let chunk_size = (g_bytes.len() - g_pos) / num_threads;

    let mut chunk_starts = Vec::with_capacity(num_threads);
    let mut chunk_ends = Vec::with_capacity(num_threads);
    let mut cur = g_pos;
    for i in 0..num_threads {
        chunk_starts.push(cur);
        if i == num_threads - 1 {
            chunk_ends.push(g_bytes.len());
        } else {
            let mut end = (cur + chunk_size).min(g_bytes.len());
            while end < g_bytes.len() && g_bytes[end - 1] != b'\n' { end += 1; }
            chunk_ends.push(end);
            cur = end;
        }
    }

    let t_build = std::time::Instant::now();
    thread::scope(|scope| {
        for tid in 0..num_threads {
            let degrees = &degrees;
            let start = chunk_starts[tid];
            let end = chunk_ends[tid];
            scope.spawn(move || {
                let ptr = g_bytes.as_ptr();
                let mut pos = start;
                while pos < end {
                    let u = unsafe { parse_u32_raw(ptr, &mut pos) };
                    degrees[u as usize].fetch_add(1, Ordering::Relaxed);
                    while pos < end && unsafe { *ptr.add(pos) } != b'\n' { pos += 1; }
                    pos += 1;
                }
            });
        }
    });

    let mut offsets = HugeBuf::<u32>::with_capacity_zeroed(n as usize + 1, use_huge);
    let mut write_pos: Vec<AtomicU32> = Vec::with_capacity(n as usize);
    let mut sum = 0u32;
    for i in 0..n as usize {
        offsets[i] = sum;
        write_pos.push(AtomicU32::new(sum));
        sum += degrees[i].load(Ordering::Relaxed);
    }
    offsets[n as usize] = sum;
    drop(degrees);

    let mut edges = HugeBuf::<Edge>::with_capacity_zeroed(m as usize, use_huge);
    let mut has_in = vec![false; n as usize];

    let edges_ptr = edges.ptr as usize;
    thread::scope(|scope| {
        for tid in 0..num_threads {
            let write_pos = &write_pos;
            let start = chunk_starts[tid];
            let end = chunk_ends[tid];
            scope.spawn(move || {
                let ptr = g_bytes.as_ptr();
                let e_ptr = edges_ptr as *mut Edge;
                let mut pos = start;
                while pos < end {
                    let u = unsafe { parse_u32_raw(ptr, &mut pos) };
                    pos += 1;
                    let v = unsafe { parse_u32_raw(ptr, &mut pos) };
                    pos += 1;
                    let w = unsafe { parse_u32_raw(ptr, &mut pos) };
                    while pos < end && unsafe { *ptr.add(pos) } != b'\n' { pos += 1; }
                    pos += 1;
                    let slot = write_pos[u as usize].fetch_add(1, Ordering::Relaxed) as usize;
                    unsafe { *e_ptr.add(slot) = Edge(v, w); }
                }
            });
        }
    });
    drop(write_pos);
    drop(g_map);

    for e in edges.iter() {
        has_in[e.0 as usize] = true;
    }
    unsafe { malloc_trim(0); }
    let build_ms = t_build.elapsed().as_secs_f64() * 1000.0;

    // Load queries
    let q_map = Mapping::open(queries_path);
    let q_bytes = q_map.bytes();
    let mut q_pos = 0;
    let mut queries = Vec::new();
    while q_pos < q_bytes.len() {
        while q_pos < q_bytes.len() && (q_bytes[q_pos] == b' ' || q_bytes[q_pos] == b'\n' || q_bytes[q_pos] == b'\r') {
            q_pos += 1;
        }
        if q_pos >= q_bytes.len() { break; }
        let s = unsafe { parse_u32_raw(q_bytes.as_ptr(), &mut q_pos) };
        while q_bytes[q_pos] == b' ' || q_bytes[q_pos] == b'\t' { q_pos += 1; }
        let t = unsafe { parse_u32_raw(q_bytes.as_ptr(), &mut q_pos) };
        while q_pos < q_bytes.len() && q_bytes[q_pos] != b'\n' { q_pos += 1; }
        queries.push((s, t));
    }
    drop(q_map);

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

    let total_settled = AtomicU64::new(0);
    let t_query = std::time::Instant::now();
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
                let edges = &edges;
                let total_settled = &total_settled;

                scope.spawn(move || {
                    let mut compact = CompactDist::new();
                    let mut heap = QuadHeap::new();
                    let mut local_settled = 0u64;

                    loop {
                        let widx = work_idx.fetch_add(1, Ordering::Relaxed);
                        if widx >= to_solve.len() { break; }
                        let idx = to_solve[widx];
                        let (s, t) = queries[idx];
                        let mut ans = INF;
                        let mut need_fallback = false;

                        compact.try_relax(s, 0);
                        heap.push(0, s);

                        while let Some((d, u)) = heap.pop() {
                            let cur = compact.get(u);
                            if cur == INF32 || d > cur as u64 { continue; }
                            if count_settled { local_settled += 1; }
                            if u == t { ans = d; break; }
                            let start = offsets[u as usize] as usize;
                            let end = offsets[u as usize + 1] as usize;
                            for i in start..end {
                                let e = unsafe { edges.get_unchecked(i) };
                                let nd = d + e.1 as u64;
                                if nd >= INF32 as u64 { need_fallback = true; break; }
                                match compact.try_relax(e.0, nd as u32) {
                                    Some(true) => {
                                        heap.push(nd, e.0);
                                        let v_off = unsafe { *offsets.get_unchecked(e.0 as usize) as usize };
                                        unsafe { std::arch::x86_64::_mm_prefetch(edges.as_ptr().add(v_off) as *const i8, std::arch::x86_64::_MM_HINT_T0); }
                                    }
                                    Some(false) => {}
                                    None => { need_fallback = true; break; }
                                }
                            }
                            if need_fallback { break; }
                        }

                        compact.clear_and_shrink();
                        heap.clear_and_shrink();

                        if need_fallback {
                            ans = fallback_dijkstra(s, t, n, offsets, edges, &mut heap);
                        }

                        results[idx].store(ans, Ordering::Relaxed);
                    }
                    if count_settled {
                        total_settled.fetch_add(local_settled, Ordering::Relaxed);
                    }
                });
            }
        });
    }

    let query_ms = t_query.elapsed().as_secs_f64() * 1000.0;
    let total_ms = t_total.elapsed().as_secs_f64() * 1000.0;
    let settled = total_settled.load(Ordering::Relaxed);

    (build_ms, query_ms, total_ms, settled)
}

fn main() {
    unsafe {
        mallopt(-1, 65536); // M_TRIM_THRESHOLD
        mallopt(-3, 65536); // M_MMAP_THRESHOLD
        mallopt(-8, 2);     // M_ARENA_MAX
    }

    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("Usage: {} <graph.txt> <queries.txt>", args[0]);
        std::process::exit(1);
    }
    let graph_path = Path::new(&args[1]);
    let queries_path = Path::new(&args[2]);

    println!("=== MADV_HUGEPAGE PROTOTYPE EVALUATION ===");

    // 1. Warm run / Baseline at 1 worker
    println!("\n--- 1-Worker Evaluation (Settled Node Rate) ---");
    let (_, b_q1, _, b_settled) = run_pipeline(graph_path, queries_path, false, 1, true);
    let b_ns_per_node = (b_q1 * 1e6) / (b_settled as f64);
    println!("Standard (4KB pages)  : Query = {:.2} ms | Settled = {} | Rate = {:.1} ns/node", b_q1, b_settled, b_ns_per_node);

    let (_, h_q1, _, h_settled) = run_pipeline(graph_path, queries_path, true, 1, true);
    let h_ns_per_node = (h_q1 * 1e6) / (h_settled as f64);
    println!("Hugepage (MADV_HUGEPAGE): Query = {:.2} ms | Settled = {} | Rate = {:.1} ns/node", h_q1, h_settled, h_ns_per_node);
    println!("1-Worker Rate Delta: {:.1} ns/node ({:+.2}%)", h_ns_per_node - b_ns_per_node, (h_ns_per_node - b_ns_per_node) / b_ns_per_node * 100.0);

    // 2. 12-Worker Evaluation
    println!("\n--- 12-Worker Evaluation (Total & Query Time) ---");
    let (b_b12, b_q12, b_t12, _) = run_pipeline(graph_path, queries_path, false, 12, false);
    println!("Standard (4KB pages)  : Build = {:.2} ms | Query = {:.2} ms | Total = {:.2} ms", b_b12, b_q12, b_t12);

    let (h_b12, h_q12, h_t12, _) = run_pipeline(graph_path, queries_path, true, 12, false);
    println!("Hugepage (MADV_HUGEPAGE): Build = {:.2} ms | Query = {:.2} ms | Total = {:.2} ms", h_b12, h_q12, h_t12);
    println!("12-Worker Total Delta : {:+.2} ms ({:+.2}%)", h_t12 - b_t12, (h_t12 - b_t12) / b_t12 * 100.0);
    println!("12-Worker Query Delta : {:+.2} ms ({:+.2}%)", h_q12 - b_q12, (h_q12 - b_q12) / b_q12 * 100.0);
}
