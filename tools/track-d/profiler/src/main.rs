use std::path::Path;
use std::sync::atomic::{AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::thread;
use std::time::Instant;

const INF: u64 = u64::MAX;
const EMPTY_SLOT: u32 = u32::MAX;
const INF32: u32 = u32::MAX;
const INIT_CAP: usize = 16384;
const MAX_CAP: usize = 524288;

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
            return Self {
                ptr: std::ptr::NonNull::<u8>::dangling().as_ptr(),
                len: 0,
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
        // MAP_PRIVATE = 2, MAP_POPULATE = 0x08000
        let flags = if populate { 2 | 0x08000 } else { 2 };
        let ptr = unsafe { mmap(std::ptr::null_mut(), len, 1, flags, file.as_raw_fd(), 0) };
        assert_ne!(ptr as isize, -1, "mmap failed");
        Self {
            ptr: ptr.cast(),
            len,
        }
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
            unsafe extern "C" {
                fn munmap(addr: *mut std::ffi::c_void, length: usize) -> i32;
            }
            unsafe {
                munmap(self.ptr.cast(), self.len);
            }
        }
    }
}

#[inline(always)]
unsafe fn parse_u32_raw(ptr: *const u8, pos: &mut usize) -> u32 {
    let mut n = (*ptr.add(*pos) - b'0') as u32;
    *pos += 1;
    loop {
        let b = *ptr.add(*pos);
        if b < b'0' || b > b'9' {
            break;
        }
        n = n * 10 + (b - b'0') as u32;
        *pos += 1;
    }
    n
}

#[derive(Copy, Clone)]
#[repr(C)]
struct Edge {
    target: u32,
    weight: u32,
}

#[derive(Copy, Clone)]
struct Slot {
    node: u32,
    dist: u32,
}

struct CompactDist {
    slots: Vec<Slot>,
    mask: usize,
    touched: Vec<usize>,
}

impl CompactDist {
    fn new(cap: usize) -> Self {
        Self {
            slots: vec![Slot { node: EMPTY_SLOT, dist: INF32 }; cap],
            mask: cap - 1,
            touched: Vec::with_capacity(1024),
        }
    }

    #[inline(always)]
    fn get(&self, node: u32) -> u32 {
        let mut idx = (node as usize).wrapping_mul(0x9E3779B9) & self.mask;
        loop {
            let slot = unsafe { self.slots.get_unchecked(idx) };
            if slot.node == node {
                return slot.dist;
            }
            if slot.node == EMPTY_SLOT {
                return INF32;
            }
            idx = (idx + 1) & self.mask;
        }
    }

    fn grow(&mut self) {
        let new_cap = self.slots.len() * 2;
        let new_mask = new_cap - 1;
        let mut new_slots = vec![Slot { node: EMPTY_SLOT, dist: INF32 }; new_cap];
        let mut new_touched = Vec::with_capacity(self.touched.len() * 2);
        for &idx in &self.touched {
            let slot = self.slots[idx];
            let mut nidx = (slot.node as usize).wrapping_mul(0x9E3779B9) & new_mask;
            loop {
                let target = unsafe { new_slots.get_unchecked_mut(nidx) };
                if target.node == EMPTY_SLOT {
                    *target = slot;
                    new_touched.push(nidx);
                    break;
                }
                nidx = (nidx + 1) & new_mask;
            }
        }
        self.slots = new_slots;
        self.mask = new_mask;
        self.touched = new_touched;
    }

    #[inline(always)]
    fn try_relax(&mut self, node: u32, new_dist: u32) -> Option<bool> {
        if self.touched.len() * 2 >= self.mask {
            if self.slots.len() < MAX_CAP {
                self.grow();
            } else {
                return None;
            }
        }
        let mut idx = (node as usize).wrapping_mul(0x9E3779B9) & self.mask;
        loop {
            let slot = unsafe { self.slots.get_unchecked_mut(idx) };
            if slot.node == node {
                if new_dist < slot.dist {
                    slot.dist = new_dist;
                    return Some(true);
                }
                return Some(false);
            }
            if slot.node == EMPTY_SLOT {
                slot.node = node;
                slot.dist = new_dist;
                self.touched.push(idx);
                return Some(true);
            }
            idx = (idx + 1) & self.mask;
        }
    }

    #[inline(always)]
    fn clear(&mut self) {
        for &idx in &self.touched {
            unsafe {
                let slot = self.slots.get_unchecked_mut(idx);
                slot.node = EMPTY_SLOT;
                slot.dist = INF32;
            }
        }
        self.touched.clear();
    }
}

#[derive(Copy, Clone)]
struct HeapItem {
    dist: u64,
    node: u32,
}

struct QuadHeap {
    data: Vec<HeapItem>,
}

impl QuadHeap {
    fn new() -> Self {
        Self {
            data: Vec::with_capacity(1024),
        }
    }

    #[inline(always)]
    fn push(&mut self, dist: u64, node: u32) {
        let mut idx = self.data.len();
        self.data.push(HeapItem { dist, node });
        while idx > 0 {
            let parent = (idx - 1) >> 2;
            if self.data[idx].dist < self.data[parent].dist {
                self.data.swap(idx, parent);
                idx = parent;
            } else {
                break;
            }
        }
    }

    #[inline(always)]
    fn pop(&mut self) -> Option<(u64, u32)> {
        if self.data.is_empty() {
            return None;
        }
        let res = self.data.swap_remove(0);
        if !self.data.is_empty() {
            let len = self.data.len();
            let mut idx = 0;
            loop {
                let child_base = (idx << 2) + 1;
                if child_base >= len {
                    break;
                }
                let child_end = (child_base + 4).min(len);
                let mut min_child = child_base;
                let mut min_dist = self.data[child_base].dist;
                for c in (child_base + 1)..child_end {
                    let d = self.data[c].dist;
                    if d < min_dist {
                        min_dist = d;
                        min_child = c;
                    }
                }
                if min_dist < self.data[idx].dist {
                    self.data.swap(idx, min_child);
                    idx = min_child;
                } else {
                    break;
                }
            }
        }
        Some((res.dist, res.node))
    }

    #[inline(always)]
    fn clear(&mut self) {
        self.data.clear();
    }
}

fn fallback_dijkstra(
    s: u32,
    t: u32,
    n: u32,
    offsets: &[u32],
    edges: &[Edge],
    heap: &mut QuadHeap,
) -> u64 {
    let mut dist = vec![INF; n as usize];
    heap.clear();
    dist[s as usize] = 0;
    heap.push(0u64, s);

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
            let edge = unsafe { edges.get_unchecked(i) };
            let nd = d + edge.weight as u64;
            if nd < dist[edge.target as usize] {
                dist[edge.target as usize] = nd;
                heap.push(nd, edge.target);
            }
        }
    }
    heap.clear();
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

fn log_smaps_breakdown() {
    if let Ok(content) = std::fs::read_to_string("/proc/self/maps") {
        let mut total_anon_kb = 0usize;
        let mut count_anon = 0usize;
        let mut anon_mappings = Vec::new();
        for line in content.lines() {
            let parts = line.split_whitespace().collect::<Vec<_>>();
            if parts.len() < 5 { continue; }
            let range = parts[0];
            let perms = parts[1];
            let path = if parts.len() >= 6 { parts[5] } else { "[anon]" };
            let (start_str, end_str) = range.split_once('-').unwrap_or(("", ""));
            let start = usize::from_str_radix(start_str, 16).unwrap_or(0);
            let end = usize::from_str_radix(end_str, 16).unwrap_or(0);
            let size_kb = (end - start) / 1024;
            if path == "[anon]" || path.starts_with("[stack") || path == "[heap]" {
                total_anon_kb += size_kb;
                count_anon += 1;
                anon_mappings.push((size_kb, perms, path));
            }
        }
        anon_mappings.sort_by(|a, b| b.0.cmp(&a.0));
        eprintln!("ANON BREAKDOWN: count={} total_virtual_anon={:.2}MB", count_anon, (total_anon_kb as f64) / 1024.0);
        for (kb, perms, name) in anon_mappings.iter().take(15) {
            eprintln!("  mapping: {:.2}MB ({:5} kB) perms={} name={}", (*kb as f64) / 1024.0, kb, perms, name);
        }
    }
}

fn main() {
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
    while pos < g_data.len() && g_data[pos] != b'\n' {
        pos += 1;
    }
    let header = &g_data[..pos];
    if pos < g_data.len() {
        pos += 1;
    }

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
        while p < edges_data.len() && edges_data[p - 1] != b'\n' {
            p += 1;
        }
        bounds.push(p);
    }
    bounds.push(edges_data.len());

    // Step A: Shared atomic degree array (one single 7 MB array)
    let degrees = (0..n).map(|_| AtomicU32::new(0)).collect::<Vec<_>>();

    // Pass 1: Parse u and count degrees, skip rest of line
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
                    if slice[cur] == b'\n' || slice[cur] == b'\r' {
                        cur += 1;
                        continue;
                    }
                    let u = unsafe { parse_u32_raw(ptr, &mut cur) };
                    unsafe {
                        degrees.get_unchecked(u as usize).fetch_add(1, Ordering::Relaxed);
                    }
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
    let pass1_time = t_pass1_start.elapsed();
    mem_samples.push(sample_mem("03_post_pass1"));

    // Prefix sum on the single 7 MB array
    let t_prefix_start = Instant::now();
    let mut offsets = vec![0u32; n as usize + 1];
    let mut sum = 0u32;
    for u in 0..n as usize {
        offsets[u] = sum;
        sum += degrees[u].load(Ordering::Relaxed);
    }
    offsets[n as usize] = sum;
    let prefix_time = t_prefix_start.elapsed();

    // Drop degrees, create write_pos from offsets
    drop(degrees);
    let write_pos = (0..n).map(|u| AtomicU32::new(offsets[u as usize])).collect::<Vec<_>>();
    mem_samples.push(sample_mem("04_post_prefix"));

    // Interleaved edges array (40 MB)
    let mut edges = vec![Edge { target: 0, weight: 0 }; sum as usize];
    let edges_ptr = edges.as_mut_ptr() as usize;

    // Pass 2: Parse u, v, w and scatter directly into interleaved CSR
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
                    let p = unsafe {
                        write_pos.get_unchecked(u as usize).fetch_add(1, Ordering::Relaxed)
                    };
                    unsafe {
                        *e_ptr.add(p as usize) = Edge { target: v, weight: w };
                    }
                }
            });
        }
    });
    let pass2_time = t_pass2_start.elapsed();
    drop(write_pos);
    mem_samples.push(sample_mem("05_post_pass2"));

    let mut has_in = vec![false; n as usize];
    for e in &edges {
        has_in[e.target as usize] = true;
    }

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

    let num_queries = queries.len();
    let results = (0..num_queries)
        .map(|_| AtomicU64::new(INF))
        .collect::<Vec<_>>();

    let mut to_solve = Vec::new();
    for (idx, &(s, t)) in queries.iter().enumerate() {
        if s == t {
            results[idx].store(0, Ordering::Relaxed);
        } else if offsets[s as usize + 1] == offsets[s as usize] || !has_in[t as usize] {
            // Unreachable
        } else {
            to_solve.push(idx);
        }
    }
    let parse_build_time = t_parse_start.elapsed();
    mem_samples.push(sample_mem("06_post_prep"));

    let t_queries_start = Instant::now();
    let total_settled = AtomicU64::new(0);
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
                    let mut compact = CompactDist::new(INIT_CAP);
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
                        heap.push(0u64, s);

                        while let Some((d, u)) = heap.pop() {
                            local_settled += 1;
                            let cur = compact.get(u);
                            if cur == INF32 || d > cur as u64 {
                                continue;
                            }
                            if u == t {
                                ans = d;
                                break;
                            }
                            let start = offsets[u as usize] as usize;
                            let end = offsets[u as usize + 1] as usize;
                            for i in start..end {
                                let edge = unsafe { edges.get_unchecked(i) };
                                let nd = d + edge.weight as u64;
                                if nd >= INF32 as u64 {
                                    need_fallback = true;
                                    break;
                                }
                                match compact.try_relax(edge.target, nd as u32) {
                                    Some(true) => heap.push(nd, edge.target),
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

                        compact.clear();
                        heap.clear();

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
    let queries_time = t_queries_start.elapsed();
    mem_samples.push(sample_mem("07_post_queries"));

    let t_out_start = Instant::now();
    let mut out_buf = Vec::with_capacity(num_queries * 12);
    for i in 0..num_queries {
        let ans = results[i].load(Ordering::Relaxed);
        if ans == INF {
            out_buf.extend_from_slice(b"-1\n");
        } else if ans == 0 {
            out_buf.extend_from_slice(b"0\n");
        } else {
            let mut digits = [0u8; 20];
            let mut at = digits.len();
            let mut val = ans;
            while val > 0 {
                at -= 1;
                digits[at] = b'0' + (val % 10) as u8;
                val /= 10;
            }
            out_buf.extend_from_slice(&digits[at..]);
            out_buf.push(b'\n');
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

    eprintln!("\n=== MEMORY PROFILE ACROSS PHASES (workers={}) ===", num_threads);
    eprintln!("{:<16} | {:>14} | {:>14} | {:>12} | {:>12} | {:>12}", "Phase", "cgroup.cur(MB)", "cgroup.peak(MB)", "RSS(MB)", "Anon(MB)", "File(MB)");
    eprintln!("{:-<16}-+-{:-<14}-+-{:-<14}-+-{:-<12}-+-{:-<12}-+-{:-<12}", "", "", "", "", "", "");
    for s in &mem_samples {
        eprintln!(
            "{:<16} | {:>14.2} | {:>14.2} | {:>12.2} | {:>12.2} | {:>12.2}",
            s.phase, s.cgroup_cur_mb, s.cgroup_peak_mb, s.rss_mb, s.anon_mb, s.file_mb
        );
    }

    log_smaps_breakdown();

    drop(graph_map);
    drop(queries_map);
    std::process::exit(0);
}
