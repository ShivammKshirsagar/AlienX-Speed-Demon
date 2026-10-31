use std::collections::{BinaryHeap, VecDeque};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::time::Instant;

const INF: u64 = u64::MAX;

#[derive(Copy, Clone, Debug)]
struct Edge {
    target: u32,
    weight: u32,
}

#[derive(Copy, Clone, Eq, PartialEq)]
struct HeapItem(u64, u32);

impl Ord for HeapItem {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        other.0.cmp(&self.0).then_with(|| other.1.cmp(&self.1))
    }
}

impl PartialOrd for HeapItem {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

struct CsrGraph {
    n: u32,
    offsets: Vec<u32>,
    edges: Vec<Edge>,
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

fn build_csr(n: u32, edges: &[(u32, u32, u32)]) -> CsrGraph {
    let mut degrees = vec![0u32; n as usize];
    for &(u, _, _) in edges {
        degrees[u as usize] += 1;
    }
    let mut offsets = vec![0u32; n as usize + 1];
    let mut sum = 0u32;
    for i in 0..n as usize {
        offsets[i] = sum;
        sum += degrees[i];
    }
    offsets[n as usize] = sum;

    let mut pos = offsets.clone();
    let mut out_edges = vec![Edge { target: 0, weight: 0 }; sum as usize];
    for &(u, v, w) in edges {
        let p = pos[u as usize];
        pos[u as usize] += 1;
        out_edges[p as usize] = Edge { target: v, weight: w };
    }

    CsrGraph {
        n,
        offsets,
        edges: out_edges,
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

fn run_dijkstra_1worker(
    graph: &CsrGraph,
    queries: &[(u32, u32)],
) -> (f64, usize, Vec<u64>) {
    let n = graph.n as usize;
    let mut dist = vec![INF; n];
    let mut touched = Vec::with_capacity(16384);
    let mut heap = BinaryHeap::with_capacity(16384);

    let mut has_in = vec![false; n];
    for e in &graph.edges {
        has_in[e.target as usize] = true;
    }

    let t0 = Instant::now();
    let mut total_settled = 0usize;
    let mut results = Vec::with_capacity(queries.len());

    for &(s, t) in queries {
        if s == t {
            results.push(0);
            continue;
        }
        if graph.offsets[s as usize + 1] == graph.offsets[s as usize] || !has_in[t as usize] {
            results.push(INF);
            continue;
        }

        dist[s as usize] = 0;
        touched.push(s as usize);
        heap.push(HeapItem(0, s));

        let mut ans = INF;
        while let Some(HeapItem(d, u)) = heap.pop() {
            if d > dist[u as usize] {
                continue;
            }
            total_settled += 1;
            if u == t {
                ans = d;
                break;
            }

            let start = graph.offsets[u as usize] as usize;
            let end = graph.offsets[u as usize + 1] as usize;
            for i in start..end {
                let e = &graph.edges[i];
                let nd = d + e.weight as u64;
                if nd < dist[e.target as usize] {
                    if dist[e.target as usize] == INF {
                        touched.push(e.target as usize);
                    }
                    dist[e.target as usize] = nd;
                    heap.push(HeapItem(nd, e.target));
                }
            }
        }

        for &idx in &touched {
            dist[idx] = INF;
        }
        touched.clear();
        heap.clear();

        results.push(ans);
    }

    let elapsed = t0.elapsed().as_secs_f64();
    (elapsed, total_settled, results)
}

fn compute_bfs_renumbering(n: u32, raw_edges: &[(u32, u32, u32)]) -> Vec<u32> {
    // Build adjacency for BFS (undirected neighborhood for maximum clustering)
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

    let mut new_id = vec![u32::MAX; n as usize];
    let mut cur_id = 0u32;
    let mut queue = VecDeque::with_capacity(65536);

    // Find node with highest degree as seed
    let mut max_deg_node = 0;
    let mut max_deg = 0;
    for i in 0..n as usize {
        let d = adj_offsets[i + 1] - adj_offsets[i];
        if d > max_deg {
            max_deg = d;
            max_deg_node = i;
        }
    }

    // Traverse starting with the highest-degree connected component
    for pass in 0..2 {
        let start_range: Box<dyn Iterator<Item = usize>> = if pass == 0 {
            Box::new(std::iter::once(max_deg_node))
        } else {
            Box::new(0..n as usize)
        };

        for seed in start_range {
            if new_id[seed] != u32::MAX {
                continue;
            }
            new_id[seed] = cur_id;
            cur_id += 1;
            queue.push_back(seed as u32);

            while let Some(u) = queue.pop_front() {
                let start = adj_offsets[u as usize] as usize;
                let end = adj_offsets[u as usize + 1] as usize;
                for i in start..end {
                    let v = adj[i] as usize;
                    if new_id[v] == u32::MAX {
                        new_id[v] = cur_id;
                        cur_id += 1;
                        queue.push_back(v as u32);
                    }
                }
            }
        }
    }

    new_id
}

fn main() {
    let mut args = std::env::args().skip(1);
    let graph_path = args.next().unwrap_or_else(|| "/data/graph.txt".into());
    let queries_path = args.next().unwrap_or_else(|| "/data/queries.txt".into());

    println!("=== BFS Renumbering Prototype (1-Worker Evaluation) ===");
    println!("Loading raw graph from {}...", graph_path);
    let (n, _m, raw_edges) = load_raw_graph(Path::new(&graph_path));
    println!("Graph loaded: n={}, m={}", n, raw_edges.len());

    let queries = load_queries(Path::new(&queries_path));
    println!("Queries loaded: {}", queries.len());

    println!("\n--- 1. Building original CSR ---");
    let orig_graph = build_csr(n, &raw_edges);

    println!("--- 2. Benchmarking original graph (1 worker) ---");
    let (orig_time, orig_settled, orig_results) = run_dijkstra_1worker(&orig_graph, &queries);
    let orig_ns_per_node = (orig_time * 1e9) / (orig_settled as f64);
    println!(
        "Original: time={:.3}s, settled={}, ns/settled={:.1}",
        orig_time, orig_settled, orig_ns_per_node
    );

    println!("\n--- 3. Computing BFS node renumbering ---");
    let t_renumber = Instant::now();
    let new_id = compute_bfs_renumbering(n, &raw_edges);
    let renumber_time = t_renumber.elapsed().as_secs_f64();
    println!("BFS renumbering computed in: {:.3}s", renumber_time);

    println!("--- 4. Building renumbered CSR ---");
    let t_rebuild = Instant::now();
    let renumbered_edges: Vec<(u32, u32, u32)> = raw_edges
        .iter()
        .map(|&(u, v, w)| (new_id[u as usize], new_id[v as usize], w))
        .collect();
    let renumbered_graph = build_csr(n, &renumbered_edges);
    println!("Renumbered CSR built in: {:.3}s", t_rebuild.elapsed().as_secs_f64());

    let renumbered_queries: Vec<(u32, u32)> = queries
        .iter()
        .map(|&(s, t)| (new_id[s as usize], new_id[t as usize]))
        .collect();

    println!("--- 5. Benchmarking BFS-renumbered graph (1 worker) ---");
    let (renum_time, renum_settled, renum_results) = run_dijkstra_1worker(&renumbered_graph, &renumbered_queries);
    let renum_ns_per_node = (renum_time * 1e9) / (renum_settled as f64);
    println!(
        "Renumbered: time={:.3}s, settled={}, ns/settled={:.1}",
        renum_time, renum_settled, renum_ns_per_node
    );

    // Verify byte-level correctness
    let mut mismatches = 0;
    for i in 0..queries.len() {
        if orig_results[i] != renum_results[i] {
            mismatches += 1;
        }
    }
    println!("\n--- Verification ---");
    println!("Mismatches between original and renumbered results: {}", mismatches);
    assert_eq!(mismatches, 0, "Renumbered distances must match original exactly!");

    println!("\n--- Summary & Comparison ---");
    println!("Original 1-worker query time:   {:.3} s ({:.1} ns/settled node)", orig_time, orig_ns_per_node);
    println!("Renumbered 1-worker query time: {:.3} s ({:.1} ns/settled node)", renum_time, renum_ns_per_node);
    let speedup = (orig_time - renum_time) / orig_time * 100.0;
    println!("Query time change:              {:.1}% speedup (factor {:.2}x)", speedup, orig_time / renum_time);
    println!("Renumbering calculation overhead: {:.3} s", renumber_time);
    let net_time_1worker = renumber_time + renum_time;
    println!("Total 1-worker time (renumber + query): {:.3} s vs original query {:.3} s", net_time_1worker, orig_time);
}
