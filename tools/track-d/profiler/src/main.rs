use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::thread;
use std::time::Instant;

const INF: u64 = u64::MAX;
const EMPTY_SLOT: u32 = u32::MAX;
const INF32: u32 = u32::MAX;
const INIT_CAP: usize = 16384;
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
    fn open(path: &Path, populate: bool) -> Self {
        use std::os::unix::io::AsRawFd;
        let file = std::fs::File::open(path).expect("open input");
        let len = file.metadata().expect("input metadata").len() as usize;
        if len == 0 {
            return Self { ptr: std::ptr::NonNull::<u8>::dangling().as_ptr(), len: 0 };
        }
        let flags = if populate { 2 | 0x08000 } else { 2 };
        let ptr = unsafe { mmap(std::ptr::null_mut(), len, 1, flags, file.as_raw_fd(), 0) };
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
    init_cap: usize,
    shrink_thresh: usize,
}

impl CompactDist {
    fn new(init_cap: usize, shrink_thresh: usize) -> Self {
        Self {
            slots: vec![Slot(EMPTY_SLOT, INF32); init_cap],
            mask: init_cap - 1,
            touched: Vec::with_capacity(1024),
            init_cap,
            shrink_thresh,
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
        if self.slots.len() >= self.shrink_thresh {
            self.slots = vec![Slot(EMPTY_SLOT, INF32); self.init_cap];
            self.mask = self.init_cap - 1;
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

#[derive(Debug, Clone)]
struct MemSample {
    phase: &'static str,
    cgroup_cur_mb: f64,
    cgroup_peak_mb: f64,
    rss_mb: f64,
    anon_mb: f64,
    file_mb: f64,
}

fn sample_mem(phase: &'static str) -> MemSample {
    let cur_b = std::fs::read_to_string("/sys/fs/cgroup/memory.current")
        .ok()
        .and_then(|s| s.trim().parse::<u64>().ok())
        .unwrap_or(0);
    let peak_b = std::fs::read_to_string("/sys/fs/cgroup/memory.peak")
        .ok()
        .and_then(|s| s.trim().parse::<u64>().ok())
        .unwrap_or(0);

    let mut rss_kb = 0u64;
    let mut anon_kb = 0u64;
    let mut file_kb = 0u64;

    if let Ok(content) = std::fs::read_to_string("/proc/self/smaps_rollup") {
        for line in content.lines() {
            if line.starts_with("Rss:") {
                rss_kb = line.split_whitespace().nth(1).and_then(|s| s.parse().ok()).unwrap_or(0);
            } else if line.starts_with("Pss_Anon:") || line.starts_with("Anonymous:") {
                if anon_kb == 0 {
                    anon_kb = line.split_whitespace().nth(1).and_then(|s| s.parse().ok()).unwrap_or(0);
                }
            } else if line.starts_with("Pss_File:") {
                file_kb = line.split_whitespace().nth(1).and_then(|s| s.parse().ok()).unwrap_or(0);
            }
        }
    }

    MemSample {
        phase,
        cgroup_cur_mb: (cur_b as f64) / (1024.0 * 1024.0),
        cgroup_peak_mb: (peak_b as f64) / (1024.0 * 1024.0),
        rss_mb: (rss_kb as f64) / 1024.0,
        anon_mb: (anon_kb as f64) / 1024.0,
        file_mb: (file_kb as f64) / 1024.0,
    }
}

fn main() {
    unsafe {
        mallopt(-1, 65536); // M_TRIM_THRESHOLD
        mallopt(-3, 65536); // M_MMAP_THRESHOLD
        mallopt(-8, 2);     // M_ARENA_MAX
    }

    let mut mem_samples = Vec::new();
    mem_samples.push(sample_mem("01_startup"));

    let start_total = Instant::now();
    let mut args = std::env::args_os().skip(1);
    let graph_path = args.next().unwrap_or_else(|| "/data/graph.txt".into());
    let queries_path = args.next().unwrap_or_else(|| "/data/queries.txt".into());
    let output_path = args.next().unwrap_or_else(|| "/out/distances.txt".into());

    let worker_override: usize = std::env::var("PROFILE_WORKERS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);

    let use_populate = std::env::var("USE_POPULATE").is_ok();

    let t0 = Instant::now();
    let graph_map = Mapping::open(Path::new(&graph_path), use_populate);
    let queries_map = Mapping::open(Path::new(&queries_path), false);
    let mmap_time = t0.elapsed();
    mem_samples.push(sample_mem("02_post_mmap"));

    let t_parse_start = Instant::now();
    let g_data = graph_map.bytes();
    let mut pos = 0;
    while pos < g_data.len() && g_data[pos] != b'\n' { pos += 1; }
    let header = &g_data[..pos];
    if pos < g_data.len() { pos += 1; }

    let mut h_pos = 0;
    let h_ptr = header.as_ptr();
    let n = unsafe { parse_u32_raw(h_ptr, &mut h_pos) };
    h_pos += 1;
    let m = unsafe { parse_u32_raw(h_ptr, &mut h_pos) };

    let edges_data = &g_data[pos..];
    let num_threads = if worker_override > 0 {
        worker_override
    } else {
        thread::available_parallelism()
            .map_or(1, usize::from)
            .min(12)
            .min(m.max(1) as usize)
    };

    let mut bounds = Vec::with_capacity(num_threads + 1);
    bounds.push(0);
    for i in 1..num_threads {
        let mut p = edges_data.len() * i / num_threads;
        while p < edges_data.len() && edges_data[p - 1] != b'\n' { p += 1; }
        bounds.push(p);
    }
    bounds.push(edges_data.len());

    let degrees = (0..n).map(|_| AtomicU32::new(0)).collect::<Vec<_>>();

    let t_pass1_start = Instant::now();
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
                    if slice[cur] == b'\n' || slice[cur] == b'\r' { cur += 1; continue; }
                    let u = unsafe { parse_u32_raw(ptr, &mut cur) };
                    let deg = unsafe { degrees.get_unchecked(u as usize) };
                    deg.fetch_add(1, Ordering::Relaxed);
                    while cur < slice.len() && slice[cur] != b'\n' { cur += 1; }
                    if cur < slice.len() { cur += 1; }
                }
            });
        }
    });
    let pass1_time = t_pass1_start.elapsed();
    mem_samples.push(sample_mem("03_post_pass1"));

    let t_prefix_start = Instant::now();
    let mut offsets = vec![0u32; n as usize + 1];
    let mut sum = 0u32;
    for u in 0..n as usize {
        offsets[u] = sum;
        sum += degrees[u].load(Ordering::Relaxed);
    }
    offsets[n as usize] = sum;
    let prefix_time = t_prefix_start.elapsed();
    drop(degrees);

    let write_pos = (0..n).map(|u| AtomicU32::new(offsets[u as usize])).collect::<Vec<_>>();
    mem_samples.push(sample_mem("04_post_prefix"));

    let mut edges = vec![Edge(0, 0); sum as usize];
    let edges_ptr = edges.as_mut_ptr() as usize;

    let t_pass2_start = Instant::now();
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
                    if slice[cur] == b'\n' || slice[cur] == b'\r' { cur += 1; continue; }
                    let u = unsafe { parse_u32_raw(ptr, &mut cur) };
                    cur += 1;
                    let v = unsafe { parse_u32_raw(ptr, &mut cur) };
                    cur += 1;
                    let w = unsafe { parse_u32_raw(ptr, &mut cur) };
                    while cur < slice.len() && (slice[cur] == b'\r' || slice[cur] == b'\n') { cur += 1; }
                    let wp = unsafe { write_pos.get_unchecked(u as usize) };
                    let p = wp.fetch_add(1, Ordering::Relaxed);
                    unsafe { *e_ptr.add(p as usize) = Edge(v, w); }
                }
            });
        }
    });
    let pass2_time = t_pass2_start.elapsed();
    drop(write_pos);
    unsafe { malloc_trim(0); }
    mem_samples.push(sample_mem("05_post_pass2"));

    let mut has_in = vec![false; n as usize];
    for e in &edges { has_in[e.0 as usize] = true; }

    let q_data = queries_map.bytes();
    let q_ptr = q_data.as_ptr();
    let mut queries = Vec::new();
    let mut q_pos = 0;
    while q_pos < q_data.len() {
        while q_pos < q_data.len() && (q_data[q_pos] == b'\n' || q_data[q_pos] == b'\r') { q_pos += 1; }
        if q_pos >= q_data.len() { break; }
        let s = unsafe { parse_u32_raw(q_ptr, &mut q_pos) };
        q_pos += 1;
        let t = unsafe { parse_u32_raw(q_ptr, &mut q_pos) };
        while q_pos < q_data.len() && (q_data[q_pos] == b'\r' || q_data[q_pos] == b'\n') { q_pos += 1; }
        queries.push((s, t));
    }

    let num_queries = queries.len();
    let results = (0..num_queries).map(|_| AtomicU64::new(INF)).collect::<Vec<_>>();

    let mut to_solve = Vec::new();
    for (idx, &(s, t)) in queries.iter().enumerate() {
        if s == t {
            results[idx].store(0, Ordering::Relaxed);
        } else if offsets[s as usize + 1] != offsets[s as usize] && has_in[t as usize] {
            to_solve.push(idx);
        }
    }
    let parse_build_time = t_parse_start.elapsed();
    mem_samples.push(sample_mem("06_post_prep"));

    let t_queries_start = Instant::now();
    let total_settled = AtomicU64::new(0);
    let peak_query_anon_kb = std::sync::Arc::new(AtomicU64::new(0));
    let peak_query_cur_b = std::sync::Arc::new(AtomicU64::new(0));
    if !to_solve.is_empty() {
        let work_idx = AtomicUsize::new(0);
        let q_workers = if worker_override > 0 {
            worker_override
        } else {
            thread::available_parallelism()
                .map_or(1, usize::from)
                .min(12)
                .min(to_solve.len())
        };

        let query_done = std::sync::Arc::new(AtomicBool::new(false));
        let q_done_sampler = query_done.clone();
        let peak_anon_sampler = peak_query_anon_kb.clone();
        let peak_cur_sampler = peak_query_cur_b.clone();

        let sampler = thread::spawn(move || {
            while !q_done_sampler.load(Ordering::Relaxed) {
                if let Ok(content) = std::fs::read_to_string("/proc/self/smaps_rollup") {
                    let mut anon_kb = 0u64;
                    for line in content.lines() {
                        if line.starts_with("Pss_Anon:") || line.starts_with("Anonymous:") {
                            if anon_kb == 0 {
                                anon_kb = line.split_whitespace().nth(1).and_then(|s| s.parse().ok()).unwrap_or(0);
                            }
                        }
                    }
                    peak_anon_sampler.fetch_max(anon_kb, Ordering::Relaxed);
                }
                if let Ok(s) = std::fs::read_to_string("/sys/fs/cgroup/memory.current") {
                    if let Ok(b) = s.trim().parse::<u64>() {
                        peak_cur_sampler.fetch_max(b, Ordering::Relaxed);
                    }
                }
                thread::sleep(std::time::Duration::from_millis(5));
            }
        });

        let init_cap = std::env::var("INIT_CAP").ok().and_then(|s| s.parse().ok()).unwrap_or(16384);
        let shrink_thresh = std::env::var("SHRINK_THRESH").ok().and_then(|s| s.parse().ok()).unwrap_or(16384);

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
                    let mut compact = CompactDist::new(init_cap, shrink_thresh);
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
                            local_settled += 1;
                            let cur = compact.get(u);
                            if cur == INF32 || d > cur as u64 { continue; }
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
                                        unsafe {
                                            std::arch::x86_64::_mm_prefetch(
                                                edges.as_ptr().add(v_off) as *const i8,
                                                std::arch::x86_64::_MM_HINT_T0,
                                            );
                                        }
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
                    total_settled.fetch_add(local_settled, Ordering::Relaxed);
                });
            }
        });
        query_done.store(true, Ordering::Relaxed);
        let _ = sampler.join();
    }
    let queries_time = t_queries_start.elapsed();
    unsafe { malloc_trim(0); }
    mem_samples.push(sample_mem("07_post_queries"));

    let t_out_start = Instant::now();
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
                if v == 0 { break; }
            }
            out_buf.extend_from_slice(&d[p..]);
        }
    }

    if let Some(parent) = Path::new(&output_path).parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    std::fs::write(&output_path, &out_buf).expect("write output");
    let out_time = t_out_start.elapsed();
    mem_samples.push(sample_mem("08_post_output"));

    let total_time = start_total.elapsed();
    let tot_settled = total_settled.load(Ordering::Relaxed);
    let ns_per_settled = if tot_settled > 0 {
        (queries_time.as_secs_f64() * 1e9 * (num_threads as f64)) / (tot_settled as f64)
    } else {
        0.0
    };

    eprintln!(
        "PROFILE: workers={} populate={} mmap={:.2}ms build={:.2}ms (pass1={:.2}ms prefix={:.2}ms pass2={:.2}ms) queries={:.2}ms output={:.2}ms total={:.2}ms | settled={} ({:.1} ns/settled CPU time)",
        num_threads,
        use_populate,
        mmap_time.as_secs_f64() * 1000.0,
        parse_build_time.as_secs_f64() * 1000.0,
        pass1_time.as_secs_f64() * 1000.0,
        prefix_time.as_secs_f64() * 1000.0,
        pass2_time.as_secs_f64() * 1000.0,
        queries_time.as_secs_f64() * 1000.0,
        out_time.as_secs_f64() * 1000.0,
        total_time.as_secs_f64() * 1000.0,
        tot_settled,
        ns_per_settled
    );

    let peak_anon_mb = (peak_query_anon_kb.load(Ordering::Relaxed) as f64) / 1024.0;
    let peak_cur_mb = (peak_query_cur_b.load(Ordering::Relaxed) as f64) / (1024.0 * 1024.0);
    eprintln!(
        "\nQUERY_SAMPLER_PEAK: workers={} peak_anon={:.2}MB peak_cgroup_cur={:.2}MB",
        num_threads, peak_anon_mb, peak_cur_mb
    );

    eprintln!("\n=== MEMORY PROFILE ACROSS PHASES (workers={}) ===", num_threads);
    eprintln!("{:<16} | {:>14} | {:>14} | {:>12} | {:>12} | {:>12}", "Phase", "cgroup.cur(MB)", "cgroup.peak(MB)", "RSS(MB)", "Anon(MB)", "File(MB)");
    eprintln!("{:-<16}-+-{:-<14}-+-{:-<14}-+-{:-<12}-+-{:-<12}-+-{:-<12}", "", "", "", "", "", "");
    for s in &mem_samples {
        eprintln!(
            "{:<16} | {:>14.2} | {:>14.2} | {:>12.2} | {:>12.2} | {:>12.2}",
            s.phase, s.cgroup_cur_mb, s.cgroup_peak_mb, s.rss_mb, s.anon_mb, s.file_mb
        );
    }

    drop(graph_map);
    drop(queries_map);
    std::process::exit(0);
}
