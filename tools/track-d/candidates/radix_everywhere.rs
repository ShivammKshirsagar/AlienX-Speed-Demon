use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};
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

struct RadixHeap {
    buckets: [Vec<(u64, u32)>; 65],
    last: u64,
    size: usize,
    non_empty: u128,
}

impl RadixHeap {
    fn new() -> Self {
        Self {
            buckets: std::array::from_fn(|_| Vec::with_capacity(64)),
            last: 0,
            size: 0,
            non_empty: 0,
        }
    }

    #[inline(always)]
    fn push(&mut self, d: u64, u: u32) {
        let x = d ^ self.last;
        let b = if x == 0 {
            0
        } else {
            64 - x.leading_zeros() as usize
        };
        self.buckets[b].push((d, u));
        self.non_empty |= 1u128 << b;
        self.size += 1;
    }

    #[inline(always)]
    fn pop(&mut self) -> Option<(u64, u32)> {
        if self.size == 0 {
            return None;
        }
        if (self.non_empty & 1) != 0 {
            let res = self.buckets[0].pop().unwrap();
            if self.buckets[0].is_empty() {
                self.non_empty &= !1u128;
            }
            self.size -= 1;
            return Some(res);
        }

        let b = self.non_empty.trailing_zeros() as usize;
        let mut min_idx = 0;
        let mut min_d = self.buckets[b][0].0;
        for i in 1..self.buckets[b].len() {
            if self.buckets[b][i].0 < min_d {
                min_d = self.buckets[b][i].0;
                min_idx = i;
            }
        }
        let res = self.buckets[b].swap_remove(min_idx);
        self.last = res.0;
        self.non_empty &= !(1u128 << b);

        let mut to_redistribute = std::mem::take(&mut self.buckets[b]);
        for (d, u) in to_redistribute.drain(..) {
            let x = d ^ self.last;
            let new_b = if x == 0 {
                0
            } else {
                64 - x.leading_zeros() as usize
            };
            self.buckets[new_b].push((d, u));
            self.non_empty |= 1u128 << new_b;
        }
        self.buckets[b] = to_redistribute;

        self.size -= 1;
        Some(res)
    }

    #[inline(always)]
    fn clear_and_shrink(&mut self) {
        let mut mask = self.non_empty;
        while mask != 0 {
            let b = mask.trailing_zeros() as usize;
            self.buckets[b].clear();
            mask &= mask - 1;
        }
        for b in &mut self.buckets {
            if b.capacity() > 4096 {
                b.shrink_to(1024);
            }
        }
        self.last = 0;
        self.size = 0;
        self.non_empty = 0;
    }
}

static FALLBACK_LOCK: AtomicBool = AtomicBool::new(false);

fn fallback_dijkstra(
    s: u32,
    t: u32,
    n: u32,
    offsets: &[u32],
    edges: &[u32],
    w_bits: u32,
    w_mask: u32,
    escape_w: u32,
    heap: &mut RadixHeap,
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
        let mut i = offsets[u as usize] as usize;
        let end = offsets[u as usize + 1] as usize;
        while i < end {
            let p = unsafe { *edges.get_unchecked(i) };
            let v = p >> w_bits;
            let wf = p & w_mask;
            let (w, step) = if wf != escape_w {
                (wf as u64, 1)
            } else {
                (unsafe { *edges.get_unchecked(i + 1) as u64 }, 2)
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

fn main() {
    unsafe {
        mallopt(-1, 65536);
        mallopt(-3, 65536);
        mallopt(-8, 2);
    }

    let mut args = std::env::args_os().skip(1);
    let graph_path = args.next().unwrap_or_else(|| "/data/graph.txt".into());
    let queries_path = args.next().unwrap_or_else(|| "/data/queries.txt".into());
    let output_path = args.next().unwrap_or_else(|| "/out/distances.txt".into());

    let graph_map = Mapping::open(Path::new(&graph_path));
    let queries_map = Mapping::open(Path::new(&queries_path));

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
    let num_threads = thread::available_parallelism()
        .map_or(1, usize::from)
        .min(12)
        .min(m.max(1) as usize);

    let mut bounds = Vec::with_capacity(num_threads + 1);
    bounds.push(0);
    for i in 1..num_threads {
        let mut p = edges_data.len() * i / num_threads;
        while p < edges_data.len() && edges_data[p] != b'\n' {
            p += 1;
        }
        if p < edges_data.len() {
            p += 1;
        }
        bounds.push(p);
    }
    bounds.push(edges_data.len());

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
                    cur += 1;
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
                    let deg = unsafe { degrees.get_unchecked(u as usize) };
                    deg.fetch_add(if w < escape_w { 1 } else { 2 }, Ordering::Relaxed);
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
    let mut edges = vec![0u32; sum as usize];
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
                let e_ptr = edges_ptr as *mut u32;
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
                    if w < escape_w {
                        let p = wp.fetch_add(1, Ordering::Relaxed) as usize;
                        unsafe {
                            *e_ptr.add(p) = (v << w_bits) | w;
                        }
                    } else {
                        let p = wp.fetch_add(2, Ordering::Relaxed) as usize;
                        unsafe {
                            *e_ptr.add(p) = (v << w_bits) | escape_w;
                            *e_ptr.add(p + 1) = w;
                        }
                    }
                }
            });
        }
    });
    drop(write_pos);
    unsafe {
        malloc_trim(0);
    }

    let mut has_in = vec![false; n as usize];
    let mut i = 0;
    while i < edges.len() {
        let p = edges[i];
        let v = (p >> w_bits) as usize;
        if v < n as usize {
            has_in[v] = true;
        }
        i += if (p & w_mask) == escape_w { 2 } else { 1 };
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
        } else if offsets[s as usize + 1] != offsets[s as usize] && has_in[t as usize] {
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
                    let mut compact = CompactDist::new();
                    let mut heap = RadixHeap::new();

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
                            if let Some(&(_, next_u)) = heap.buckets[0].last() {
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
                            if u == t {
                                ans = d;
                                break;
                            }
                            let mut i = offsets[u as usize] as usize;
                            let end = offsets[u as usize + 1] as usize;
                            while i < end {
                                let p = unsafe { *edges.get_unchecked(i) };
                                let v = p >> w_bits;
                                let wf = p & w_mask;
                                let (w, step) = if wf != escape_w {
                                    (wf as u64, 1)
                                } else {
                                    (unsafe { *edges.get_unchecked(i + 1) as u64 }, 2)
                                };
                                i += step;
                                let nd = d + w;
                                if nd >= INF32 as u64 {
                                    need_fallback = true;
                                    break;
                                }
                                match compact.try_relax(v, nd as u32) {
                                    Some(true) => {
                                        heap.push(nd, v);
                                        unsafe {
                                            std::arch::x86_64::_mm_prefetch(
                                                offsets.as_ptr().add(v as usize) as *const i8,
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
                            ans = fallback_dijkstra(
                                s, t, n, offsets, edges, w_bits, w_mask, escape_w, &mut heap,
                            );
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

    if let Some(parent) = Path::new(&output_path).parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    std::fs::write(&output_path, &out_buf).expect("write output");

    drop(graph_map);
    drop(queries_map);
    std::process::exit(0);
}
