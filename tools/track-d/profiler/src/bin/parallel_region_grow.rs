use std::collections::VecDeque;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::thread;
use std::time::Instant;

const INF: u64 = u64::MAX;
const EMPTY_SLOT: u32 = u32::MAX;
const INF32: u32 = u32::MAX;
const INIT_CAP: usize = 16384;
const MAX_CAP: usize = 524288;
const EMPTY_NODE: u32 = u32::MAX;

#[derive(Copy, Clone, Debug)]
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
        if self.slots.len() > INIT_CAP {
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
    FALLBACK_LOCK.store(false, Ordering::Release);
    ans
}

struct CsrGraph {
    n: u32,
    offsets: Vec<u32>,
    edges: Vec<Edge>,
    has_in: Vec<bool>,
}

fn load_raw_graph(path: &Path) -> (u32, u32, Vec<(u32, u32, u32)>) {
    let file = File::open(path).expect("open graph");
    let mut reader = BufReader::new(file);
    let mut line = String::new();
    reader.read_line(&mut line).expect("read header");
    let parts: Vec<u32> = line
        .split_whitespace()
        .map(|s| s.parse().expect("parse header"))
        .collect();
    let n = parts[0];
    let m = parts[1];

    let mut raw_edges = Vec::with_capacity(m as usize);
    line.clear();
    while reader.read_line(&mut line).expect("read edge") > 0 {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 3 {
            let u: u32 = parts[0].parse().unwrap();
            let v: u32 = parts[1].parse().unwrap();
            let w: u32 = parts[2].parse().unwrap();
            raw_edges.push((u, v, w));
        }
        line.clear();
    }
    (n, m, raw_edges)
}

fn build_csr_from_edges(n: u32, edges: &[(u32, u32, u32)]) -> CsrGraph {
    let mut degrees = vec![0u32; n as usize];
    let mut has_in = vec![false; n as usize];
    for &(u, v, _) in edges {
        degrees[u as usize] += 1;
        has_in[v as usize] = true;
    }
    let mut offsets = vec![0u32; n as usize + 1];
    let mut sum = 0u32;
    for i in 0..n as usize {
        offsets[i] = sum;
        sum += degrees[i];
    }
    offsets[n as usize] = sum;

    let mut pos = offsets.clone();
    let mut out_edges = vec![Edge(0, 0); sum as usize];
    for &(u, v, w) in edges {
        let p = pos[u as usize];
        pos[u as usize] += 1;
        out_edges[p as usize] = Edge(v, w);
    }

    CsrGraph {
        n,
        offsets,
        edges: out_edges,
        has_in,
    }
}

fn load_queries(path: &Path) -> Vec<(u32, u32)> {
    let file = File::open(path).expect("open queries");
    let reader = BufReader::new(file);
    let mut queries = Vec::new();
    for line in reader.lines() {
        let line = line.expect("read query line");
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 2 {
            let s: u32 = parts[0].parse().unwrap();
            let t: u32 = parts[1].parse().unwrap();
            queries.push((s, t));
        }
    }
    queries
}

fn run_dijkstra_12workers(
    graph: &CsrGraph,
    queries: &[(u32, u32)],
) -> (f64, Vec<u64>) {
    let n = graph.n;
    let offsets = &graph.offsets;
    let edges = &graph.edges;
    let has_in = &graph.has_in;

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

    let t0 = Instant::now();
    if !to_solve.is_empty() {
        let work_idx = AtomicUsize::new(0);
        let q_workers = 12.min(to_solve.len());

        thread::scope(|scope| {
            for _ in 0..q_workers {
                let work_idx = &work_idx;
                let to_solve = &to_solve;
                let queries = &queries;
                let results = &results;
                let offsets = &offsets;
                let edges = &edges;

                scope.spawn(move || {
                    let mut compact = CompactDist::new();
                    let mut heap = QuadHeap::new();

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
                            if u == t { ans = d; break; }
                            let start = offsets[u as usize] as usize;
                            let end = offsets[u as usize + 1] as usize;
                            for i in start..end {
                                let e = unsafe { edges.get_unchecked(i) };
                                let nd = d + e.1 as u64;
                                if nd >= INF32 as u64 { need_fallback = true; break; }
                                match compact.try_relax(e.0, nd as u32) {
                                    Some(true) => heap.push(nd, e.0),
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
                });
            }
        });
    }
    let elapsed = t0.elapsed().as_secs_f64();
    let final_res = results.iter().map(|a| a.load(Ordering::Relaxed)).collect();
    (elapsed, final_res)
}

// Parallel Region-Growing Node Ordering
fn compute_parallel_region_ordering(
    n: u32,
    raw_edges: &[(u32, u32, u32)],
    num_regions: usize,
) -> (Vec<u32>, f64, usize) {
    let t0 = Instant::now();

    // 1. Build adjacency list for neighborhood expansion
    let mut deg = vec![0u32; n as usize];
    for &(u, v, _) in raw_edges {
        deg[u as usize] += 1;
        deg[v as usize] += 1;
    }
    let mut adj_offsets = vec![0u32; n as usize + 1];
    let mut sum = 0u32;
    for i in 0..n as usize {
        adj_offsets[i] = sum;
        sum += deg[i];
    }
    adj_offsets[n as usize] = sum;

    let mut adj_pos = adj_offsets.clone();
    let mut adj = vec![0u32; sum as usize];
    for &(u, v, _) in raw_edges {
        let pu = adj_pos[u as usize];
        adj_pos[u as usize] += 1;
        adj[pu as usize] = v;

        let pv = adj_pos[v as usize];
        adj_pos[v as usize] += 1;
        adj[pv as usize] = u;
    }

    // 2. Select `num_regions` well-separated seeds
    // Use high-degree nodes spaced across the network
    let mut seeds = Vec::with_capacity(num_regions);
    let mut step = (n as usize) / (num_regions * 2);
    for r in 0..num_regions {
        let candidate = (r * step * 2) as u32;
        seeds.push(candidate.min(n - 1));
    }

    // 3. Parallel Region Growing via BFS
    let region = (0..n).map(|_| AtomicU32::new(EMPTY_NODE)).collect::<Vec<_>>();
    let mut region_nodes: Vec<Vec<u32>> = (0..num_regions)
        .map(|_| Vec::with_capacity((n as usize) / num_regions + 1024))
        .collect();

    let adj_offsets_ref = &adj_offsets;
    let adj_ref = &adj;
    let region_ref = &region;

    // Mutex-free parallel expansion: each thread owns its region and its local output buffer
    thread::scope(|scope| {
        for (r_id, r_nodes) in region_nodes.iter_mut().enumerate() {
            let seed = seeds[r_id];
            scope.spawn(move || {
                let mut queue = VecDeque::with_capacity(32768);
                if region_ref[seed as usize]
                    .compare_exchange(EMPTY_NODE, r_id as u32, Ordering::Relaxed, Ordering::Relaxed)
                    .is_ok()
                {
                    queue.push_back(seed);
                    r_nodes.push(seed);
                }

                while let Some(u) = queue.pop_front() {
                    let start = adj_offsets_ref[u as usize] as usize;
                    let end = adj_offsets_ref[u as usize + 1] as usize;
                    for i in start..end {
                        let v = adj_ref[i];
                        if region_ref[v as usize]
                            .compare_exchange(EMPTY_NODE, r_id as u32, Ordering::Relaxed, Ordering::Relaxed)
                            .is_ok()
                        {
                            queue.push_back(v);
                            r_nodes.push(v);
                        }
                    }
                }
            });
        }
    });

    // 4. Assign remaining nodes (disconnected islands, isolated nodes)
    let mut unassigned = Vec::new();
    for u in 0..n {
        if region[u as usize].load(Ordering::Relaxed) == EMPTY_NODE {
            unassigned.push(u);
        }
    }

    // 5. Build contiguous permutation: new_id[u]
    let mut new_id = vec![0u32; n as usize];
    let mut cur_id = 0u32;
    for r_nodes in &region_nodes {
        for &u in r_nodes {
            new_id[u as usize] = cur_id;
            cur_id += 1;
        }
    }
    for &u in &unassigned {
        new_id[u as usize] = cur_id;
        cur_id += 1;
    }

    let elapsed = t0.elapsed().as_secs_f64();
    // Extra memory: region array (4B*n) + new_id (4B*n) + region_nodes buffers (4B*n) + adj (4B*2m) + adj_offsets (4B*n)
    let extra_mem_bytes = (n as usize) * 4 * 4 + (sum as usize) * 4;

    (new_id, elapsed, extra_mem_bytes)
}

fn main() {
    let mut args = std::env::args().skip(1);
    let graph_path = args.next().unwrap_or_else(|| "/data/graph.txt".into());
    let queries_path = args.next().unwrap_or_else(|| "/data/queries.txt".into());

    println!("=== Parallel Region-Growing Prototype (12 Workers) ===");
    println!("Loading raw graph from {}...", graph_path);
    let (n, _m, mut raw_edges) = load_raw_graph(Path::new(&graph_path));
    println!("Graph loaded: n={}, m={}", n, raw_edges.len());

    let queries = load_queries(Path::new(&queries_path));
    println!("Queries loaded: {}", queries.len());

    println!("\n--- 1. Current Code Baseline (12 Workers) ---");
    let t_base_build = Instant::now();
    let base_graph = build_csr_from_edges(n, &raw_edges);
    let base_build_time = t_base_build.elapsed().as_secs_f64();
    println!("Baseline CSR build: {:.3}s", base_build_time);

    let (base_query_time, base_results) = run_dijkstra_12workers(&base_graph, &queries);
    println!("Baseline 12-worker query time: {:.3}s", base_query_time);
    let base_total_time = base_build_time + base_query_time;
    println!("Baseline total (build + query): {:.3}s", base_total_time);

    // Drop baseline CSR to free 47 MB immediately
    drop(base_graph);

    println!("\n--- 2. Parallel Region-Growing Node Ordering (12 Regions) ---");
    let num_regions = 12;
    let (new_id, renumber_time, extra_mem_bytes) = compute_parallel_region_ordering(n, &raw_edges, num_regions);
    println!("Region-growing ordering computed in: {:.3}s (cur_id={}, n={})", renumber_time, new_id.iter().max().unwrap_or(&0) + 1, n);
    println!("Extra memory for region growing: {:.2} MB", extra_mem_bytes as f64 / (1024.0 * 1024.0));

    println!("\n--- 3. Applying Permutation to CSR ---");
    let t_perm_build = Instant::now();
    println!("Applying permutation in-place to {} edges...", raw_edges.len());
    // In-place mapping: 0 MB extra memory!
    for edge in &mut raw_edges {
        edge.0 = new_id[edge.0 as usize];
        edge.1 = new_id[edge.1 as usize];
    }
    println!("In-place mapping done in {:.3}s, building permuted CSR...", t_perm_build.elapsed().as_secs_f64());
    let perm_graph = build_csr_from_edges(n, &raw_edges);
    let perm_build_time = t_perm_build.elapsed().as_secs_f64();
    println!("Permutation applied & CSR built in: {:.3}s", perm_build_time);

    let perm_queries: Vec<(u32, u32)> = queries
        .iter()
        .map(|&(s, t)| (new_id[s as usize], new_id[t as usize]))
        .collect();

    println!("\n--- 4. Benchmarking Renumbered Graph (12 Workers) ---");
    let (perm_query_time, perm_results) = run_dijkstra_12workers(&perm_graph, &perm_queries);
    println!("Renumbered 12-worker query time: {:.3}s", perm_query_time);

    // Verification
    let mut mismatches = 0;
    for i in 0..queries.len() {
        if base_results[i] != perm_results[i] {
            mismatches += 1;
        }
    }
    println!("\n--- Verification ---");
    println!("Mismatches against baseline: {}", mismatches);
    assert_eq!(mismatches, 0, "All query distances must match byte-for-byte!");

    println!("\n=== NET 12-WORKER TIME COMPARISON ===");
    println!("Current Code (Baseline):");
    println!("  - CSR Build Time:       {:.3} s", base_build_time);
    println!("  - 12-Worker Query Time: {:.3} s", base_query_time);
    println!("  - Net Total:            {:.3} s", base_total_time);

    let net_region_time = renumber_time + perm_build_time + perm_query_time;
    println!("\nRegion-Growing Approach:");
    println!("  - Renumbering Time:     {:.3} s", renumber_time);
    println!("  - Permuted Build Time:  {:.3} s", perm_build_time);
    println!("  - 12-Worker Query Time: {:.3} s (vs baseline {:.3} s, {:.1}% speedup on queries)",
        perm_query_time, base_query_time, (base_query_time - perm_query_time) / base_query_time * 100.0);
    println!("  - Net Total (All-in):   {:.3} s", net_region_time);

    println!("\nNet Comparison:");
    if net_region_time < base_total_time {
        println!("  NET WIN: Region-growing is {:.3} s faster ({:.1}% net speedup)",
            base_total_time - net_region_time, (base_total_time - net_region_time) / base_total_time * 100.0);
    } else {
        println!("  NET SLOWER: Region-growing is {:.3} s slower (+{:.1}% net time) because renumbering overhead ({:.3}s) exceeds query savings ({:.3}s)",
            net_region_time - base_total_time, (net_region_time - base_total_time) / base_total_time * 100.0,
            renumber_time + perm_build_time - base_build_time, base_query_time - perm_query_time);
    }
}
