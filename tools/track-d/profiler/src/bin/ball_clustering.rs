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

fn fallback_dijkstra(
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

fn main() {
    unsafe {
        mallopt(-1, 65536); // M_TRIM_THRESHOLD
        mallopt(-3, 65536); // M_MMAP_THRESHOLD
        mallopt(-8, 2); // M_ARENA_MAX
    }

    let args: Vec<String> = std::env::args().collect();
    if args.len() < 4 {
        eprintln!(
            "Usage: {} <graph.txt> <queries.txt> <out.txt> [ball_size] [workers]",
            args[0]
        );
        std::process::exit(1);
    }
    let graph_path = Path::new(&args[1]);
    let queries_path = Path::new(&args[2]);
    let out_path = Path::new(&args[3]);

    let ball_size: usize = args.get(4).and_then(|s| s.parse().ok()).unwrap_or(256);
    let workers: usize = args
        .get(5)
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

    // 1. Initial CSR Build
    let t_init_build_start = std::time::Instant::now();
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
    let degrees: Vec<AtomicU32> = (0..n).map(|_| AtomicU32::new(0)).collect();

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
                    let deg = unsafe { degrees.get_unchecked(u as usize) };
                    deg.fetch_add(1, Ordering::Relaxed);
                    while cur < slice.len() && slice[cur] != b'\n' {
                        cur += 1;
                    }
                    if cur < slice.len() {
                        cur += 1;
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
    let mut edges = vec![Edge(0, 0); sum as usize];
    let edges_ptr = edges.as_mut_ptr() as usize;

    thread::scope(|scope| {
        for i in 0..num_threads {
            let start = bounds[i];
            let end = bounds[i + 1];
            let write_pos = &write_pos;
            scope.spawn(move || {
                let slice = &edges_data[start..end];
                let ptr = slice.as_ptr();
                let mut cur = 0;
                let e_ptr = edges_ptr as *mut Edge;
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
                    let p = wp.fetch_add(1, Ordering::Relaxed);
                    unsafe {
                        *e_ptr.add(p as usize) = Edge(v, w);
                    }
                }
            });
        }
    });
    drop(write_pos);
    drop(graph_map);
    let t_init_build_ms = t_init_build_start.elapsed().as_secs_f64() * 1000.0;

    // Load Queries
    let queries_map = Mapping::open(queries_path);
    let q_data = queries_map.bytes();
    let q_ptr = q_data.as_ptr();
    let mut raw_queries = Vec::new();
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
        raw_queries.push((s, t));
    }
    drop(queries_map);

    // If ball_size == 0, skip clustering and use original CSR (baseline)
    let (final_offsets, final_edges, queries, has_in, t_order_ms, t_remap_ms, total_build_ms) =
        if ball_size == 0 {
            let mut has_in = vec![false; n as usize];
            for e in &edges {
                has_in[e.0 as usize] = true;
            }
            unsafe {
                malloc_trim(0);
            }
            (
                offsets,
                edges,
                raw_queries,
                has_in,
                0.0,
                0.0,
                t_init_build_ms,
            )
        } else {
            // 2. Parallel Ball-Clustering Node Ordering
            let t_order_start = std::time::Instant::now();
            let claimed: Vec<AtomicBool> = (0..n).map(|_| AtomicBool::new(false)).collect();
            let mut thread_nodes: Vec<Vec<u32>> = (0..num_threads).map(|_| Vec::new()).collect();

            thread::scope(|scope| {
                let node_chunk = (n as usize + num_threads - 1) / num_threads;
                for (tid, t_nodes) in thread_nodes.iter_mut().enumerate() {
                    let claimed = &claimed;
                    let offsets = &offsets;
                    let edges = &edges;
                    scope.spawn(move || {
                        let s_start = (tid * node_chunk).min(n as usize);
                        let s_end = ((tid + 1) * node_chunk).min(n as usize);
                        let mut queue = Vec::with_capacity(ball_size);

                        for u in s_start..s_end {
                            if claimed[u]
                                .compare_exchange(
                                    false,
                                    true,
                                    Ordering::Acquire,
                                    Ordering::Relaxed,
                                )
                                .is_ok()
                            {
                                let ball_start = t_nodes.len();
                                t_nodes.push(u as u32);
                                queue.clear();
                                queue.push(u as u32);
                                let mut q_head = 0;

                                while q_head < queue.len()
                                    && (t_nodes.len() - ball_start) < ball_size
                                {
                                    let curr = queue[q_head];
                                    q_head += 1;
                                    let e_start = offsets[curr as usize] as usize;
                                    let e_end = offsets[curr as usize + 1] as usize;
                                    for i in e_start..e_end {
                                        let v = edges[i].0;
                                        if claimed[v as usize]
                                            .compare_exchange(
                                                false,
                                                true,
                                                Ordering::Acquire,
                                                Ordering::Relaxed,
                                            )
                                            .is_ok()
                                        {
                                            t_nodes.push(v);
                                            queue.push(v);
                                            if (t_nodes.len() - ball_start) == ball_size {
                                                break;
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        // Sweep for any remaining unclaimed nodes in slice
                        for u in s_start..s_end {
                            if claimed[u]
                                .compare_exchange(
                                    false,
                                    true,
                                    Ordering::Acquire,
                                    Ordering::Relaxed,
                                )
                                .is_ok()
                            {
                                t_nodes.push(u as u32);
                            }
                        }
                    });
                }
            });
            drop(claimed);

            // Prefix sum across threads to get starting ID for each thread
            let mut thread_starts = Vec::with_capacity(num_threads);
            let mut id_acc = 0u32;
            for t_nodes in &thread_nodes {
                thread_starts.push(id_acc);
                id_acc += t_nodes.len() as u32;
            }
            assert_eq!(id_acc, n, "all nodes must be assigned");

            let mut old_to_new = vec![0u32; n as usize];
            let mut new_to_old = vec![0u32; n as usize];

            thread::scope(|scope| {
                for (tid, t_nodes) in thread_nodes.iter().enumerate() {
                    let start_id = thread_starts[tid];
                    let o2n_ptr = old_to_new.as_mut_ptr() as usize;
                    let n2o_ptr = new_to_old.as_mut_ptr() as usize;
                    scope.spawn(move || {
                        let o2n = o2n_ptr as *mut u32;
                        let n2o = n2o_ptr as *mut u32;
                        for (offset, &old_u) in t_nodes.iter().enumerate() {
                            let new_id = start_id + offset as u32;
                            unsafe {
                                *o2n.add(old_u as usize) = new_id;
                                *n2o.add(new_id as usize) = old_u;
                            }
                        }
                    });
                }
            });
            drop(thread_nodes);
            let t_order_ms = t_order_start.elapsed().as_secs_f64() * 1000.0;

            // 3. Copy and Remap Adjacency Lists into New Order
            let t_remap_start = std::time::Instant::now();
            let mut new_offsets = vec![0u32; n as usize + 1];
            let mut sum_deg = 0u32;
            for new_id in 0..n as usize {
                new_offsets[new_id] = sum_deg;
                let old_u = new_to_old[new_id] as usize;
                sum_deg += offsets[old_u + 1] - offsets[old_u];
            }
            new_offsets[n as usize] = sum_deg;
            assert_eq!(sum_deg, m as u32);

            let mut new_edges = vec![Edge(0, 0); m as usize];
            let new_edges_ptr = new_edges.as_mut_ptr() as usize;

            thread::scope(|scope| {
                let node_chunk = (n as usize + num_threads - 1) / num_threads;
                for tid in 0..num_threads {
                    let offsets = &offsets;
                    let edges = &edges;
                    let new_offsets = &new_offsets;
                    let new_to_old = &new_to_old;
                    let old_to_new = &old_to_new;
                    scope.spawn(move || {
                        let s_start = (tid * node_chunk).min(n as usize);
                        let s_end = ((tid + 1) * node_chunk).min(n as usize);
                        let ne_ptr = new_edges_ptr as *mut Edge;
                        for new_id in s_start..s_end {
                            let old_u = new_to_old[new_id] as usize;
                            let old_start = offsets[old_u] as usize;
                            let old_end = offsets[old_u + 1] as usize;
                            let new_start = new_offsets[new_id] as usize;
                            for k in 0..(old_end - old_start) {
                                let e = edges[old_start + k];
                                let new_v = old_to_new[e.0 as usize];
                                unsafe {
                                    *ne_ptr.add(new_start + k) = Edge(new_v, e.1);
                                }
                            }
                        }
                    });
                }
            });

            // Drop old graph buffers and temporary mapping
            drop(offsets);
            drop(edges);
            drop(new_to_old);

            let mut new_has_in = vec![false; n as usize];
            for e in &new_edges {
                new_has_in[e.0 as usize] = true;
            }
            unsafe {
                malloc_trim(0);
            }
            let t_remap_ms = t_remap_start.elapsed().as_secs_f64() * 1000.0;
            let total_build_ms = t_init_build_ms + t_order_ms + t_remap_ms;

            // Remap queries
            let remapped_queries: Vec<(u32, u32)> = raw_queries
                .iter()
                .map(|&(s, t)| (old_to_new[s as usize], old_to_new[t as usize]))
                .collect();
            drop(old_to_new);
            unsafe {
                malloc_trim(0);
            }

            (
                new_offsets,
                new_edges,
                remapped_queries,
                new_has_in,
                t_order_ms,
                t_remap_ms,
                total_build_ms,
            )
        };

    let num_queries = queries.len();
    let results: Vec<AtomicU64> = (0..num_queries).map(|_| AtomicU64::new(INF)).collect();
    let mut to_solve = Vec::with_capacity(num_queries);
    for (idx, &(s, t)) in queries.iter().enumerate() {
        if s == t {
            results[idx].store(0, Ordering::Relaxed);
        } else if final_offsets[s as usize + 1] != final_offsets[s as usize] && has_in[t as usize] {
            to_solve.push(idx);
        }
    }

    // 5. Query Phase with current two-tier prefetch query code
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
                let offsets = &final_offsets;
                let edges = &final_edges;
                let total_settled = &total_settled;

                scope.spawn(move || {
                    let mut compact = CompactDist::new();
                    let mut heap = QuadHeap::new();
                    let mut local_settled = 0u64;

                    loop {
                        let widx = work_idx.fetch_add(1, Ordering::Relaxed);
                        if widx >= to_solve.len() {
                            break;
                        }
                        let idx = to_solve[widx];
                        let (s, t) = queries[idx];
                        let mut ans = INF;
                        let mut need_fallback = false;

                        compact.try_relax(s, 0);
                        heap.push(0, s);

                        while let Some((d, u)) = heap.pop() {
                            if let Some(&HeapItem(_, next_u)) = heap.0.first() {
                                let next_off =
                                    unsafe { *offsets.get_unchecked(next_u as usize) as usize };
                                unsafe {
                                    std::arch::x86_64::_mm_prefetch(
                                        edges.as_ptr().add(next_off) as *const i8,
                                        std::arch::x86_64::_MM_HINT_T0,
                                    );
                                }
                            }
                            let cur = compact.get(u);
                            if cur == INF32 || d > cur as u64 {
                                continue;
                            }
                            local_settled += 1;
                            if u == t {
                                ans = d;
                                break;
                            }
                            let start = offsets[u as usize] as usize;
                            let end = offsets[u as usize + 1] as usize;
                            for i in start..end {
                                let e = unsafe { edges.get_unchecked(i) };
                                let nd = d + e.1 as u64;
                                if nd >= INF32 as u64 {
                                    need_fallback = true;
                                    break;
                                }
                                match compact.try_relax(e.0, nd as u32) {
                                    Some(true) => {
                                        heap.push(nd, e.0);
                                        unsafe {
                                            std::arch::x86_64::_mm_prefetch(
                                                offsets.as_ptr().add(e.0 as usize) as *const i8,
                                                std::arch::x86_64::_MM_HINT_T0,
                                            );
                                        }
                                    }
                                    Some(false) => {}
                                    None => {
                                        need_fallback = true;
                                        break;
                                    }
                                }
                            }
                            if need_fallback {
                                break;
                            }
                        }

                        compact.clear_and_shrink();
                        heap.clear_and_shrink();

                        if need_fallback {
                            ans = fallback_dijkstra(s, t, n, offsets, edges, &mut heap);
                        }

                        results[idx].store(ans, Ordering::Relaxed);
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
        "RESULT: B={} workers={} ordering={:.2}ms remap={:.2}ms build={:.2}ms query={:.2}ms total={:.2}ms peak_anon={:.2}MB settled={} ns_per_node={:.1}",
        ball_size, workers, t_order_ms, t_remap_ms, total_build_ms, query_ms, total_ms, peak_anon_mb, settled, ns_per_node
    );
}
