use std::path::Path;
use std::sync::atomic::{AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::thread;

const INF: u64 = u64::MAX;
const EMPTY_SLOT: u32 = u32::MAX;
const INF32: u32 = u32::MAX;
const INIT_CAP: usize = 16384;
const MAX_CAP: usize = 524288;

struct Mapping {
    ptr: *mut u8,
    len: usize,
}

// SAFETY: Mapping owns the mmap allocation and raw pointer, which can be safely sent across threads.
unsafe impl Send for Mapping {}
// SAFETY: Mapping provides read-only access to the mapped byte slice, safe for concurrent shared reads.
unsafe impl Sync for Mapping {}

impl Mapping {
    fn open(path: &Path) -> Self {
        use std::os::unix::io::AsRawFd;
        let file = std::fs::File::open(path).expect("open input");
        let len = file.metadata().expect("input metadata").len() as usize;
        if len == 0 {
            return Self { ptr: std::ptr::NonNull::<u8>::dangling().as_ptr(), len: 0 };
        }
        unsafe extern "C" {
            fn mmap(a: *mut std::ffi::c_void, l: usize, p: i32, f: i32, fd: i32, o: isize) -> *mut std::ffi::c_void;
        }
        // SAFETY: MAP_PRIVATE (2) with PROT_READ (1). Valid fd and length.
        let ptr = unsafe { mmap(std::ptr::null_mut(), len, 1, 2, file.as_raw_fd(), 0) };
        assert_ne!(ptr as isize, -1, "mmap failed");
        Self { ptr: ptr.cast(), len }
    }

    #[inline(always)]
    fn bytes(&self) -> &[u8] {
        if self.len == 0 { &[] } else {
            // SAFETY: self.ptr points to self.len valid readable bytes mapped from the input file.
            unsafe { std::slice::from_raw_parts(self.ptr, self.len) }
        }
    }
}

impl Drop for Mapping {
    fn drop(&mut self) {
        if self.len != 0 {
            unsafe extern "C" { fn munmap(a: *mut std::ffi::c_void, l: usize) -> i32; }
            // SAFETY: self.ptr was returned by mmap and is unmapped exactly once upon drop.
            unsafe { munmap(self.ptr.cast(), self.len); }
        }
    }
}

#[inline(always)]
unsafe fn parse_u32_raw(ptr: *const u8, pos: &mut usize) -> u32 {
    let mut n = (*ptr.add(*pos) - b'0') as u32;
    *pos += 1;
    loop {
        let b = *ptr.add(*pos);
        if b < b'0' || b > b'9' { break; }
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
            // SAFETY: idx is masked by self.mask < self.slots.len().
            let slot = unsafe { self.slots.get_unchecked(idx) };
            if slot.node == node { return slot.dist; }
            if slot.node == EMPTY_SLOT { return INF32; }
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
                // SAFETY: nidx is masked by new_mask < new_slots.len().
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
            if self.slots.len() < MAX_CAP { self.grow(); } else { return None; }
        }
        let mut idx = (node as usize).wrapping_mul(0x9E3779B9) & self.mask;
        loop {
            // SAFETY: idx is masked by self.mask < self.slots.len().
            let slot = unsafe { self.slots.get_unchecked_mut(idx) };
            if slot.node == node {
                if new_dist < slot.dist { slot.dist = new_dist; return Some(true); }
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
            // SAFETY: idx in self.touched is a valid slot index populated in try_relax/grow.
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
        Self { data: Vec::with_capacity(1024) }
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
            } else { break; }
        }
    }

    #[inline(always)]
    fn pop(&mut self) -> Option<(u64, u32)> {
        if self.data.is_empty() { return None; }
        let res = self.data.swap_remove(0);
        if !self.data.is_empty() {
            let len = self.data.len();
            let mut idx = 0;
            loop {
                let child_base = (idx << 2) + 1;
                if child_base >= len { break; }
                let child_end = (child_base + 4).min(len);
                let mut min_child = child_base;
                let mut min_dist = self.data[child_base].dist;
                for c in (child_base + 1)..child_end {
                    let d = self.data[c].dist;
                    if d < min_dist { min_dist = d; min_child = c; }
                }
                if min_dist < self.data[idx].dist {
                    self.data.swap(idx, min_child);
                    idx = min_child;
                } else { break; }
            }
        }
        Some((res.dist, res.node))
    }

    #[inline(always)]
    fn clear(&mut self) { self.data.clear(); }
}

fn fallback_dijkstra(
    s: u32, t: u32, n: u32,
    offsets: &[u32], edges: &[Edge],
    heap: &mut QuadHeap,
) -> u64 {
    let mut dist = vec![INF; n as usize];
    heap.clear();
    dist[s as usize] = 0;
    heap.push(0u64, s);

    let mut ans = INF;
    while let Some((d, u)) = heap.pop() {
        if d > dist[u as usize] { continue; }
        if u == t { ans = d; break; }
        let start = offsets[u as usize] as usize;
        let end = offsets[u as usize + 1] as usize;
        for i in start..end {
            // SAFETY: i is within offsets[u]..offsets[u + 1] <= edges.len().
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

fn main() {
    let mut args = std::env::args_os().skip(1);
    let graph_path = args.next().unwrap_or_else(|| "/data/graph.txt".into());
    let queries_path = args.next().unwrap_or_else(|| "/data/queries.txt".into());
    let output_path = args.next().unwrap_or_else(|| "/out/distances.txt".into());

    let graph_map = Mapping::open(Path::new(&graph_path));
    let queries_map = Mapping::open(Path::new(&queries_path));

    let g_data = graph_map.bytes();
    let mut pos = 0;
    while pos < g_data.len() && g_data[pos] != b'\n' { pos += 1; }
    let header = &g_data[..pos];
    if pos < g_data.len() { pos += 1; }

    let mut h_pos = 0;
    let h_ptr = header.as_ptr();
    // SAFETY: header contains ASCII 'n m' formatted digits followed by newline.
    let n = unsafe { parse_u32_raw(h_ptr, &mut h_pos) };
    h_pos += 1;
    let m = unsafe { parse_u32_raw(h_ptr, &mut h_pos) };

    let edges_data = &g_data[pos..];
    let num_threads = thread::available_parallelism()
        .map_or(1, usize::from)
        .min(12)
        .min(m.max(1) as usize);

    let mut bounds = Vec::with_capacity(num_threads + 1);
    bounds.push(0);
    for i in 1..num_threads {
        let mut p = edges_data.len() * i / num_threads;
        while p < edges_data.len() && edges_data[p - 1] != b'\n' { p += 1; }
        bounds.push(p);
    }
    bounds.push(edges_data.len());

    // Step A: Shared atomic degree array (one single 7 MB array instead of 12 x 7 MB = 84 MB)
    let degrees = (0..n).map(|_| AtomicU32::new(0)).collect::<Vec<_>>();

    // Pass 1: Parse u and count degrees, skip rest of line without intermediate allocations
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
                    // SAFETY: cur is verified < slice.len() and points to digit.
                    let u = unsafe { parse_u32_raw(ptr, &mut cur) };
                    // SAFETY: u < n as guaranteed by graph specification.
                    unsafe { degrees.get_unchecked(u as usize).fetch_add(1, Ordering::Relaxed); }
                    while cur < slice.len() && slice[cur] != b'\n' { cur += 1; }
                    if cur < slice.len() { cur += 1; }
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

    let write_pos = (0..n).map(|u| AtomicU32::new(offsets[u as usize])).collect::<Vec<_>>();
    let mut edges = vec![Edge { target: 0, weight: 0 }; sum as usize];
    let edges_ptr = edges.as_mut_ptr() as usize;

    // Pass 2: Parse u, v, w directly and scatter into interleaved Edge CSR
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
                    // SAFETY: cur is bounds-checked and points to digit.
                    let u = unsafe { parse_u32_raw(ptr, &mut cur) };
                    cur += 1;
                    let v = unsafe { parse_u32_raw(ptr, &mut cur) };
                    cur += 1;
                    let w = unsafe { parse_u32_raw(ptr, &mut cur) };
                    while cur < slice.len() && (slice[cur] == b'\r' || slice[cur] == b'\n') { cur += 1; }
                    // SAFETY: u < n guaranteed by spec; write_pos partition allocates unique slot < sum.
                    let p = unsafe { write_pos.get_unchecked(u as usize).fetch_add(1, Ordering::Relaxed) };
                    unsafe { *e_ptr.add(p as usize) = Edge { target: v, weight: w }; }
                }
            });
        }
    });
    drop(write_pos);

    let mut has_in = vec![false; n as usize];
    for e in &edges { has_in[e.target as usize] = true; }

    let q_data = queries_map.bytes();
    let q_ptr = q_data.as_ptr();
    let mut queries = Vec::new();
    let mut q_pos = 0;
    while q_pos < q_data.len() {
        while q_pos < q_data.len() && (q_data[q_pos] == b'\n' || q_data[q_pos] == b'\r') { q_pos += 1; }
        if q_pos >= q_data.len() { break; }
        // SAFETY: q_pos < q_data.len() and points to digit.
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
        } else if offsets[s as usize + 1] == offsets[s as usize] || !has_in[t as usize] {
            // Unreachable: remains INF (-1)
        } else {
            to_solve.push(idx);
        }
    }

    if !to_solve.is_empty() {
        let work_idx = AtomicUsize::new(0);
        let q_workers = thread::available_parallelism()
            .map_or(1, usize::from)
            .min(12)
            .min(to_solve.len());

        thread::scope(|scope| {
            for _ in 0..q_workers {
                let work_idx = &work_idx;
                let to_solve = &to_solve;
                let queries = &queries;
                let results = &results;
                let offsets = &offsets;
                let edges = &edges;

                scope.spawn(move || {
                    let mut compact = CompactDist::new(INIT_CAP);
                    let mut heap = QuadHeap::new();

                    loop {
                        let widx = work_idx.fetch_add(1, Ordering::Relaxed);
                        if widx >= to_solve.len() { break; }
                        let idx = to_solve[widx];
                        let (s, t) = queries[idx];
                        let mut ans = INF;
                        let mut need_fallback = false;

                        compact.try_relax(s, 0);
                        heap.push(0u64, s);

                        while let Some((d, u)) = heap.pop() {
                            let cur = compact.get(u);
                            if cur == INF32 || d > cur as u64 { continue; }
                            if u == t { ans = d; break; }
                            let start = offsets[u as usize] as usize;
                            let end = offsets[u as usize + 1] as usize;
                            for i in start..end {
                                // SAFETY: i is within valid edge range.
                                let edge = unsafe { edges.get_unchecked(i) };
                                let nd = d + edge.weight as u64;
                                if nd >= INF32 as u64 { need_fallback = true; break; }
                                match compact.try_relax(edge.target, nd as u32) {
                                    Some(true) => heap.push(nd, edge.target),
                                    Some(false) => {}
                                    None => { need_fallback = true; break; }
                                }
                            }
                            if need_fallback { break; }
                        }

                        compact.clear();
                        heap.clear();

                        if need_fallback {
                            ans = fallback_dijkstra(s, t, n, offsets, edges, &mut heap);
                        }

                        results[idx].store(ans, Ordering::Relaxed);
                    }
                });
            }
        });
    }

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

    drop(graph_map);
    drop(queries_map);
    std::process::exit(0);
}
