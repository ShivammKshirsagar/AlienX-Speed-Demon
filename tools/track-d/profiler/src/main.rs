use std::path::Path;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::thread;
use std::time::Instant;

const INF: u64 = u64::MAX;

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
        let ptr = unsafe { mmap(std::ptr::null_mut(), len, 1, 2, file.as_raw_fd(), 0) };
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
fn parse_u32_fast(bytes: &[u8], pos: &mut usize) -> u32 {
    let mut n = (bytes[*pos] - b'0') as u32;
    *pos += 1;
    while *pos < bytes.len() {
        let b = bytes[*pos];
        if b < b'0' || b > b'9' {
            break;
        }
        n = n * 10 + (b - b'0') as u32;
        *pos += 1;
    }
    n
}

fn main() {
    let start_total = Instant::now();
    let mut args = std::env::args_os().skip(1);
    let graph_path = args.next().unwrap_or_else(|| "/data/graph.txt".into());
    let queries_path = args.next().unwrap_or_else(|| "/data/queries.txt".into());
    let output_path = args.next().unwrap_or_else(|| "/out/distances.txt".into());

    let worker_override: usize = std::env::var("PROFILE_WORKERS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);

    let t0 = Instant::now();
    let graph_map = Mapping::open(Path::new(&graph_path));
    let queries_map = Mapping::open(Path::new(&queries_path));
    let mmap_time = t0.elapsed();

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
    let n = parse_u32_fast(header, &mut h_pos);
    h_pos += 1;
    let m = parse_u32_fast(header, &mut h_pos);

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

    let worker_data = thread::scope(|scope| {
        let mut joins = Vec::with_capacity(num_threads);
        for i in 0..num_threads {
            let start = bounds[i];
            let end = bounds[i + 1];
            joins.push(scope.spawn(move || {
                let slice = &edges_data[start..end];
                let mut cur = 0;
                let mut edges = Vec::with_capacity((end - start) / 16);
                let mut deg = vec![0u32; n as usize];
                while cur < slice.len() {
                    if slice[cur] == b'\n' || slice[cur] == b'\r' {
                        cur += 1;
                        continue;
                    }
                    let u = parse_u32_fast(slice, &mut cur);
                    cur += 1;
                    let v = parse_u32_fast(slice, &mut cur);
                    cur += 1;
                    let w = parse_u32_fast(slice, &mut cur);
                    while cur < slice.len() && (slice[cur] == b'\r' || slice[cur] == b'\n') {
                        cur += 1;
                    }
                    deg[u as usize] += 1;
                    edges.push((u, v, w));
                }
                (edges, deg)
            }));
        }
        joins
            .into_iter()
            .map(|j| j.join().expect("parse thread panicked"))
            .collect::<Vec<_>>()
    });
    let parse_time = t_parse_start.elapsed();

    let t_build_start = Instant::now();
    let mut offsets = vec![0u32; n as usize + 1];
    let mut sum = 0u32;
    let mut worker_partitions = worker_data;
    let t_prefix_start = Instant::now();
    for u in 0..n as usize {
        offsets[u] = sum;
        for (_, deg) in &mut worker_partitions {
            let cnt = deg[u];
            deg[u] = sum;
            sum += cnt;
        }
    }
    offsets[n as usize] = sum;
    let prefix_loop_time = t_prefix_start.elapsed();

    let mut heads = vec![0u32; sum as usize];
    let mut weights = vec![0u32; sum as usize];

    let heads_ptr = heads.as_mut_ptr() as usize;
    let weights_ptr = weights.as_mut_ptr() as usize;

    let t_scatter_start = Instant::now();
    thread::scope(|scope| {
        for (edges, mut deg) in worker_partitions {
            scope.spawn(move || {
                let h_ptr = heads_ptr as *mut u32;
                let w_ptr = weights_ptr as *mut u32;
                for (u, v, w) in edges {
                    let p = deg[u as usize];
                    deg[u as usize] += 1;
                    unsafe {
                        *h_ptr.add(p as usize) = v;
                        *w_ptr.add(p as usize) = w;
                    }
                }
            });
        }
    });
    let scatter_time = t_scatter_start.elapsed();
    let build_time = t_build_start.elapsed();

    let mut has_in = vec![false; n as usize];
    for &v in &heads {
        if (v as usize) < has_in.len() {
            has_in[v as usize] = true;
        }
    }

    let q_data = queries_map.bytes();
    let mut queries = Vec::new();
    let mut q_pos = 0;
    while q_pos < q_data.len() {
        while q_pos < q_data.len() && (q_data[q_pos] == b'\n' || q_data[q_pos] == b'\r') {
            q_pos += 1;
        }
        if q_pos >= q_data.len() {
            break;
        }
        let s = parse_u32_fast(q_data, &mut q_pos);
        q_pos += 1;
        let t = parse_u32_fast(q_data, &mut q_pos);
        while q_pos < q_data.len() && (q_data[q_pos] == b'\r' || q_data[q_pos] == b'\n') {
            q_pos += 1;
        }
        queries.push((s, t));
    }

    let t_queries_start = Instant::now();
    let num_queries = queries.len();
    let ablate_dup = std::env::var("ABLATE_DUP_CACHE").map(|v| v == "1").unwrap_or(false);

    let (unique_queries, canonical_idx) = if !ablate_dup {
        let mut uq = Vec::new();
        let mut qmap = std::collections::HashMap::new();
        let mut cidx = Vec::with_capacity(num_queries);
        for &q in &queries {
            match qmap.entry(q) {
                std::collections::hash_map::Entry::Occupied(e) => {
                    cidx.push(*e.get());
                }
                std::collections::hash_map::Entry::Vacant(e) => {
                    let u_idx = uq.len();
                    e.insert(u_idx);
                    uq.push(q);
                    cidx.push(u_idx);
                }
            }
        }
        (uq, cidx)
    } else {
        let cidx = (0..num_queries).collect::<Vec<_>>();
        (queries.clone(), cidx)
    };

    let num_unique = unique_queries.len();
    let unique_results = (0..num_unique)
        .map(|_| AtomicU64::new(INF))
        .collect::<Vec<_>>();

    let mut to_solve = Vec::new();
    for (u_idx, &(s, t)) in unique_queries.iter().enumerate() {
        if s == t {
            unique_results[u_idx].store(0, Ordering::Relaxed);
        } else if offsets[s as usize + 1] == offsets[s as usize] || !has_in[t as usize] {
            // Unreachable: remains INF (-1)
        } else {
            to_solve.push(u_idx);
        }
    }

    let total_settled = AtomicU64::new(0);

    if !to_solve.is_empty() {
        let work_idx = AtomicUsize::new(0);
        let q_workers = num_threads.min(to_solve.len());

        thread::scope(|scope| {
            for _ in 0..q_workers {
                let work_idx = &work_idx;
                let to_solve = &to_solve;
                let unique_queries = &unique_queries;
                let unique_results = &unique_results;
                let offsets = &offsets;
                let heads = &heads;
                let weights = &weights;
                let total_settled = &total_settled;

                scope.spawn(move || {
                    let mut dist = vec![INF; n as usize];
                    let mut touched = Vec::with_capacity(1024);
                    let mut heap = std::collections::BinaryHeap::new();
                    let mut local_settled: u64 = 0;

                    loop {
                        let widx = work_idx.fetch_add(1, Ordering::Relaxed);
                        if widx >= to_solve.len() {
                            break;
                        }
                        let u_idx = to_solve[widx];
                        let (s, t) = unique_queries[u_idx];
                        let mut ans = INF;

                        if s < n {
                            dist[s as usize] = 0;
                            touched.push(s);
                            heap.push(std::cmp::Reverse((0u64, s)));

                            while let Some(std::cmp::Reverse((d, u))) = heap.pop() {
                                if d > dist[u as usize] {
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
                                    let v = heads[i];
                                    let nd = d + weights[i] as u64;
                                    if nd < dist[v as usize] {
                                        if dist[v as usize] == INF {
                                            touched.push(v);
                                        }
                                        dist[v as usize] = nd;
                                        heap.push(std::cmp::Reverse((nd, v)));
                                    }
                                }
                            }
                        }

                        for &u in &touched {
                            dist[u as usize] = INF;
                        }
                        touched.clear();
                        heap.clear();

                        unique_results[u_idx].store(ans, Ordering::Relaxed);
                    }
                    total_settled.fetch_add(local_settled, Ordering::Relaxed);
                });
            }
        });
    }
    let queries_time = t_queries_start.elapsed();

    let t_out_start = Instant::now();
    let mut out_buf = Vec::with_capacity(num_queries * 12);
    for i in 0..num_queries {
        let u_idx = canonical_idx[i];
        let ans = unique_results[u_idx].load(Ordering::Relaxed);
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

    let total_time = start_total.elapsed();
    let tot_settled = total_settled.load(Ordering::Relaxed);
    let ns_per_settled = if tot_settled > 0 {
        (queries_time.as_secs_f64() * 1e9 * (num_threads as f64)) / (tot_settled as f64)
    } else {
        0.0
    };

    eprintln!(
        "PROFILE: workers={} mmap={:.2}ms parse={:.2}ms build={:.2}ms (prefix={:.2}ms scatter={:.2}ms) queries={:.2}ms output={:.2}ms total={:.2}ms | settled={} ({:.1} ns/settled CPU time)",
        num_threads,
        mmap_time.as_secs_f64() * 1000.0,
        parse_time.as_secs_f64() * 1000.0,
        build_time.as_secs_f64() * 1000.0,
        prefix_loop_time.as_secs_f64() * 1000.0,
        scatter_time.as_secs_f64() * 1000.0,
        queries_time.as_secs_f64() * 1000.0,
        out_time.as_secs_f64() * 1000.0,
        total_time.as_secs_f64() * 1000.0,
        tot_settled,
        ns_per_settled
    );

    drop(graph_map);
    drop(queries_map);
    std::process::exit(0);
}
