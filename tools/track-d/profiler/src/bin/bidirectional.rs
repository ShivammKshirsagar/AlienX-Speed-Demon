use std::collections::BinaryHeap;
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

// Simple binary heap item: (distance, node)
#[derive(Copy, Clone, Eq, PartialEq)]
struct HeapItem(u64, u32);

impl Ord for HeapItem {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Reverse for min-heap
        other.0.cmp(&self.0).then_with(|| other.1.cmp(&self.1))
    }
}

impl PartialOrd for HeapItem {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

struct Graph {
    n: u32,
    fwd_offsets: Vec<u32>,
    fwd_edges: Vec<Edge>,
    bwd_offsets: Vec<u32>,
    bwd_edges: Vec<Edge>,
}

fn load_graph(path: &Path) -> Graph {
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

    let mut fwd_degrees = vec![0u32; n as usize];
    let mut bwd_degrees = vec![0u32; n as usize];

    // Read edges
    let mut raw_edges = Vec::with_capacity(m as usize);
    line.clear();
    while reader.read_line(&mut line).expect("read edge") > 0 {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 3 {
            let u: u32 = parts[0].parse().unwrap();
            let v: u32 = parts[1].parse().unwrap();
            let w: u32 = parts[2].parse().unwrap();
            fwd_degrees[u as usize] += 1;
            bwd_degrees[v as usize] += 1;
            raw_edges.push((u, v, w));
        }
        line.clear();
    }

    let mut fwd_offsets = vec![0u32; n as usize + 1];
    let mut bwd_offsets = vec![0u32; n as usize + 1];
    let mut f_sum = 0u32;
    let mut b_sum = 0u32;
    for i in 0..n as usize {
        fwd_offsets[i] = f_sum;
        f_sum += fwd_degrees[i];
        bwd_offsets[i] = b_sum;
        b_sum += bwd_degrees[i];
    }
    fwd_offsets[n as usize] = f_sum;
    bwd_offsets[n as usize] = b_sum;

    let mut fwd_pos = fwd_offsets.clone();
    let mut bwd_pos = bwd_offsets.clone();
    let mut fwd_edges = vec![Edge { target: 0, weight: 0 }; f_sum as usize];
    let mut bwd_edges = vec![Edge { target: 0, weight: 0 }; b_sum as usize];

    for (u, v, w) in raw_edges {
        let fp = fwd_pos[u as usize];
        fwd_pos[u as usize] += 1;
        fwd_edges[fp as usize] = Edge { target: v, weight: w };

        let bp = bwd_pos[v as usize];
        bwd_pos[v as usize] += 1;
        bwd_edges[bp as usize] = Edge { target: u, weight: w };
    }

    Graph {
        n,
        fwd_offsets,
        fwd_edges,
        bwd_offsets,
        bwd_edges,
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

// Unidirectional Dijkstra for comparison
fn unidirectional_dijkstra(
    s: u32,
    t: u32,
    graph: &Graph,
    dist: &mut [u64],
    touched: &mut Vec<usize>,
    heap: &mut BinaryHeap<HeapItem>,
) -> (u64, usize) {
    if s == t {
        return (0, 0);
    }
    dist[s as usize] = 0;
    touched.push(s as usize);
    heap.push(HeapItem(0, s));

    let mut settled = 0;
    let mut ans = INF;

    while let Some(HeapItem(d, u)) = heap.pop() {
        if d > dist[u as usize] {
            continue;
        }
        settled += 1;
        if u == t {
            ans = d;
            break;
        }

        let start = graph.fwd_offsets[u as usize] as usize;
        let end = graph.fwd_offsets[u as usize + 1] as usize;
        for i in start..end {
            let e = &graph.fwd_edges[i];
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

    for &idx in touched.iter() {
        dist[idx] = INF;
    }
    touched.clear();
    heap.clear();

    (ans, settled)
}

// Bidirectional Dijkstra
fn bidirectional_dijkstra(
    s: u32,
    t: u32,
    graph: &Graph,
    dist_f: &mut [u64],
    dist_b: &mut [u64],
    touched_f: &mut Vec<usize>,
    touched_b: &mut Vec<usize>,
    heap_f: &mut BinaryHeap<HeapItem>,
    heap_b: &mut BinaryHeap<HeapItem>,
) -> (u64, usize, usize) {
    if s == t {
        return (0, 0, 0);
    }

    // Dead-end shortcuts
    if graph.fwd_offsets[s as usize + 1] == graph.fwd_offsets[s as usize] {
        return (INF, 0, 0);
    }
    if graph.bwd_offsets[t as usize + 1] == graph.bwd_offsets[t as usize] {
        return (INF, 0, 0);
    }

    dist_f[s as usize] = 0;
    touched_f.push(s as usize);
    heap_f.push(HeapItem(0, s));

    dist_b[t as usize] = 0;
    touched_b.push(t as usize);
    heap_b.push(HeapItem(0, t));

    let mut mu = INF;
    let mut settled_f = 0;
    let mut settled_b = 0;

    while !heap_f.is_empty() && !heap_b.is_empty() {
        let top_f = heap_f.peek().unwrap().0;
        let top_b = heap_b.peek().unwrap().0;

        if mu != INF && top_f.saturating_add(top_b) >= mu {
            break;
        }

        if top_f <= top_b {
            let HeapItem(d, u) = heap_f.pop().unwrap();
            if d > dist_f[u as usize] {
                continue;
            }
            settled_f += 1;

            if mu != INF && d.saturating_add(top_b) >= mu {
                continue;
            }

            let db = dist_b[u as usize];
            if db != INF {
                let total = d.saturating_add(db);
                if total < mu {
                    mu = total;
                }
            }

            let start = graph.fwd_offsets[u as usize] as usize;
            let end = graph.fwd_offsets[u as usize + 1] as usize;
            for i in start..end {
                let e = &graph.fwd_edges[i];
                let nd = d + e.weight as u64;
                if nd < dist_f[e.target as usize] {
                    if dist_f[e.target as usize] == INF {
                        touched_f.push(e.target as usize);
                    }
                    dist_f[e.target as usize] = nd;
                    heap_f.push(HeapItem(nd, e.target));

                    let v_db = dist_b[e.target as usize];
                    if v_db != INF {
                        let total = nd.saturating_add(v_db);
                        if total < mu {
                            mu = total;
                        }
                    }
                }
            }
        } else {
            let HeapItem(d, u) = heap_b.pop().unwrap();
            if d > dist_b[u as usize] {
                continue;
            }
            settled_b += 1;

            if mu != INF && d.saturating_add(top_f) >= mu {
                continue;
            }

            let df = dist_f[u as usize];
            if df != INF {
                let total = d.saturating_add(df);
                if total < mu {
                    mu = total;
                }
            }

            let start = graph.bwd_offsets[u as usize] as usize;
            let end = graph.bwd_offsets[u as usize + 1] as usize;
            for i in start..end {
                let e = &graph.bwd_edges[i];
                let nd = d + e.weight as u64;
                if nd < dist_b[e.target as usize] {
                    if dist_b[e.target as usize] == INF {
                        touched_b.push(e.target as usize);
                    }
                    dist_b[e.target as usize] = nd;
                    heap_b.push(HeapItem(nd, e.target));

                    let v_df = dist_f[e.target as usize];
                    if v_df != INF {
                        let total = nd.saturating_add(v_df);
                        if total < mu {
                            mu = total;
                        }
                    }
                }
            }
        }
    }

    for &idx in touched_f.iter() {
        dist_f[idx] = INF;
    }
    touched_f.clear();
    heap_f.clear();

    for &idx in touched_b.iter() {
        dist_b[idx] = INF;
    }
    touched_b.clear();
    heap_b.clear();

    (mu, settled_f, settled_b)
}

fn main() {
    let mut args = std::env::args().skip(1);
    let graph_path = args.next().unwrap_or_else(|| "/data/graph.txt".into());
    let queries_path = args.next().unwrap_or_else(|| "/data/queries.txt".into());
    let out_path = args.next().unwrap_or_else(|| "/out/bidirectional.txt".into());

    println!("Loading graph from {}...", graph_path);
    let t0 = Instant::now();
    let graph = load_graph(Path::new(&graph_path));
    println!("Graph loaded in {:.2}s: n={}, fwd_m={}, bwd_m={}", t0.elapsed().as_secs_f64(), graph.n, graph.fwd_edges.len(), graph.bwd_edges.len());

    println!("Loading queries from {}...", queries_path);
    let queries = load_queries(Path::new(&queries_path));
    println!("Loaded {} queries", queries.len());

    let n = graph.n as usize;
    let mut dist_uni = vec![INF; n];
    let mut touched_uni = Vec::with_capacity(16384);
    let mut heap_uni = BinaryHeap::with_capacity(16384);

    let mut dist_f = vec![INF; n];
    let mut dist_b = vec![INF; n];
    let mut touched_f = Vec::with_capacity(16384);
    let mut touched_b = Vec::with_capacity(16384);
    let mut heap_f = BinaryHeap::with_capacity(16384);
    let mut heap_b = BinaryHeap::with_capacity(16384);

    println!("\n--- Running Unidirectional Dijkstra ---");
    let t_uni = Instant::now();
    let mut uni_settled_total = 0usize;
    let mut uni_results = Vec::with_capacity(queries.len());
    for &(s, t) in &queries {
        let (ans, settled) = unidirectional_dijkstra(s, t, &graph, &mut dist_uni, &mut touched_uni, &mut heap_uni);
        uni_settled_total += settled;
        uni_results.push(ans);
    }
    let uni_elapsed = t_uni.elapsed();
    println!("Unidirectional time: {:.3}s, total settled nodes: {}, avg settled/query: {:.1}",
        uni_elapsed.as_secs_f64(), uni_settled_total, uni_settled_total as f64 / queries.len() as f64);

    println!("\n--- Running Bidirectional Dijkstra ---");
    let t_bi = Instant::now();
    let mut bi_settled_f_total = 0usize;
    let mut bi_settled_b_total = 0usize;
    let mut bi_results = Vec::with_capacity(queries.len());
    let mut mismatches = 0;

    for (idx, &(s, t)) in queries.iter().enumerate() {
        let (ans, sf, sb) = bidirectional_dijkstra(s, t, &graph, &mut dist_f, &mut dist_b, &mut touched_f, &mut touched_b, &mut heap_f, &mut heap_b);
        bi_settled_f_total += sf;
        bi_settled_b_total += sb;
        bi_results.push(ans);

        if ans != uni_results[idx] {
            mismatches += 1;
            if mismatches <= 5 {
                eprintln!("MISMATCH query {} (s={}, t={}): uni={} bi={}", idx, s, t, uni_results[idx], ans);
            }
        }
    }
    let bi_elapsed = t_bi.elapsed();
    let bi_settled_total = bi_settled_f_total + bi_settled_b_total;

    println!("Bidirectional time: {:.3}s, total settled: {} (fwd={}, bwd={}), avg settled/query: {:.1}",
        bi_elapsed.as_secs_f64(), bi_settled_total, bi_settled_f_total, bi_settled_b_total, bi_settled_total as f64 / queries.len() as f64);
    println!("Mismatches against unidirectional: {}", mismatches);
    println!("Settled nodes ratio (bi / uni): {:.2}%", (bi_settled_total as f64 / uni_settled_total as f64) * 100.0);

    // Write output to verify SHA-256
    let mut out_str = String::new();
    for &ans in &bi_results {
        if ans == INF {
            out_str.push_str("-1\n");
        } else {
            out_str.push_str(&format!("{}\n", ans));
        }
    }
    std::fs::write(&out_path, out_str).expect("write output");
    println!("Wrote output to {}", out_path);
}
