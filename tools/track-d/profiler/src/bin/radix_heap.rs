use std::fs::File;
use std::io::Read;
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
            let slot = unsafe { *self.slots.get_unchecked(i) };
            if slot.0 == node {
                return slot.1;
            }
            if slot.0 == EMPTY_SLOT {
                return INF32;
            }
            i = (i + 1) & self.mask;
        }
    }

    #[inline(always)]
    fn try_relax(&mut self, node: u32, dist: u32) -> Option<bool> {
        let mut i = (node as usize).wrapping_mul(0x9E3779B9) & self.mask;
        loop {
            let slot = unsafe { *self.slots.get_unchecked(i) };
            if slot.0 == node {
                if dist < slot.1 {
                    unsafe {
                        self.slots.get_unchecked_mut(i).1 = dist;
                    }
                    return Some(true);
                }
                return Some(false);
            }
            if slot.0 == EMPTY_SLOT {
                if self.touched.len() * 10 >= self.slots.len() * 7 {
                    if !self.grow() {
                        return None;
                    }
                    i = (node as usize).wrapping_mul(0x9E3779B9) & self.mask;
                    continue;
                }
                unsafe {
                    *self.slots.get_unchecked_mut(i) = Slot(node, dist);
                }
                self.touched.push(i);
                return Some(true);
            }
            i = (i + 1) & self.mask;
        }
    }

    #[cold]
    #[inline(never)]
    fn grow(&mut self) -> bool {
        let old_cap = self.slots.len();
        let new_cap = old_cap * 2;
        if new_cap > MAX_CAP {
            return false;
        }
        let old_slots = std::mem::replace(&mut self.slots, vec![Slot(EMPTY_SLOT, INF32); new_cap]);
        self.mask = new_cap - 1;
        let old_touched = std::mem::take(&mut self.touched);
        self.touched.reserve(old_touched.len());
        for &idx in &old_touched {
            let slot = unsafe { *old_slots.get_unchecked(idx) };
            if slot.0 != EMPTY_SLOT {
                let mut i = (slot.0 as usize).wrapping_mul(0x9E3779B9) & self.mask;
                loop {
                    let cur = unsafe { *self.slots.get_unchecked(i) };
                    if cur.0 == EMPTY_SLOT {
                        unsafe {
                            *self.slots.get_unchecked_mut(i) = slot;
                        }
                        self.touched.push(i);
                        break;
                    }
                    i = (i + 1) & self.mask;
                }
            }
        }
        true
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
        let b = if x == 0 { 0 } else { 64 - x.leading_zeros() as usize };
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
            let new_b = if x == 0 { 0 } else { 64 - x.leading_zeros() as usize };
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

fn fallback_dijkstra_quad(
    s: u32,
    t: u32,
    n: u32,
    offsets: &[u32],
    edges: &[u32],
    w_bits: u32,
    w_mask: u32,
    escape_w: u32,
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

fn fallback_dijkstra_radix(
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

fn read_anon_kb() -> Option<u64> {
    let mut s = String::new();
    File::open("/proc/self/status").ok()?.read_to_string(&mut s).ok()?;
    for line in s.lines() {
        if line.starts_with("RssAnon:") {
            let mut parts = line.split_whitespace();
            parts.next();
            return parts.next().and_then(|v| v.parse().ok());
        }
    }
    None
}

fn main() {
    unsafe {
        mallopt(-1, 65536);
        mallopt(-3, 65536);
        mallopt(-8, 2);
    }

    let args: Vec<String> = std::env::args().collect();
    if args.len() < 4 {
        eprintln!(
            "Usage: {} <graph.txt> <queries.txt> <out.txt> [heap_type: 0=QuadHeap, 1=RadixEverywhere] [workers]",
            args[0]
        );
        std::process::exit(1);
    }
    let graph_path = Path::new(&args[1]);
    let queries_path = Path::new(&args[2]);
    let out_path = Path::new(&args[3]);

    let heap_type: usize = args.get(4).and_then(|s| s.parse().ok()).unwrap_or(0);
    let workers: usize = args
        .get(5)
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(|| {
            std::env::var("WORKERS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(12)
        });

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
    let t_build_start = std::time::Instant::now();

    let graph_map = Mapping::open(graph_path);
    let queries_map = Mapping::open(queries_path);

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
    let num_threads = workers.min(m.max(1) as usize);

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
    drop(graph_map);

    let mut has_in = vec![false; n as usize];
    let mut ei = 0;
    while ei < edges.len() {
        let p = edges[ei];
        let v = (p >> w_bits) as usize;
        if v < n as usize {
            has_in[v] = true;
        }
        ei += if (p & w_mask) != escape_w { 1 } else { 2 };
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
    drop(queries_map);

    let build_ms = t_build_start.elapsed().as_secs_f64() * 1000.0;

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

    let total_settled = Arc::new(AtomicUsize::new(0));

    let t_query_start = std::time::Instant::now();

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
                let total_settled = total_settled.clone();

                scope.spawn(move || {
                    let mut compact = CompactDist::new();
                    let mut q_heap = QuadHeap::new();
                    let mut r_heap = RadixHeap::new();
                    let mut local_settled = 0;

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

                        if heap_type == 0 {
                            // QuadHeap
                            q_heap.push(0, s);
                            while let Some((d, u)) = q_heap.pop() {
                                local_settled += 1;
                                if let Some(&HeapItem(_, next_u)) = q_heap.0.first() {
                                    let next_off = unsafe { *offsets.get_unchecked(next_u as usize) as usize };
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
                                            q_heap.push(nd, v);
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
                            q_heap.clear_and_shrink();
                            compact.clear_and_shrink();

                            if need_fallback {
                                ans = fallback_dijkstra_quad(
                                    s, t, n, offsets, edges, w_bits, w_mask, escape_w, &mut q_heap,
                                );
                            }
                        } else {
                            // RadixHeap Everywhere (64-bit distances)
                            r_heap.push(0, s);
                            while let Some((d, u)) = r_heap.pop() {
                                local_settled += 1;
                                if let Some(&(_, next_u)) = r_heap.buckets[0].last() {
                                    let next_off = unsafe { *offsets.get_unchecked(next_u as usize) as usize };
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
                                            r_heap.push(nd, v);
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
                            r_heap.clear_and_shrink();
                            compact.clear_and_shrink();

                            if need_fallback {
                                ans = fallback_dijkstra_radix(
                                    s, t, n, offsets, edges, w_bits, w_mask, escape_w, &mut r_heap,
                                );
                            }
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

    sampler_stop.store(true, Ordering::Relaxed);
    let _ = sampler_handle.join();
    let peak_anon_mb = (peak_anon_kb.load(Ordering::Relaxed) as f64) / 1024.0;

    let settled_nodes = total_settled.load(Ordering::Relaxed);
    let ns_per_node = if settled_nodes > 0 {
        (query_ms * 1_000_000.0) / (settled_nodes as f64)
    } else {
        0.0
    };

    use std::io::Write;
    let mut out_file = std::io::BufWriter::new(File::create(out_path).expect("create out"));
    for res in &results {
        let ans = res.load(Ordering::Relaxed);
        if ans == INF {
            writeln!(out_file, "-1").unwrap();
        } else {
            writeln!(out_file, "{}", ans).unwrap();
        }
    }
    out_file.flush().unwrap();

    eprintln!(
        "RESULT: heap_type={} workers={} build={:.2}ms query={:.2}ms total={:.2}ms peak_anon={:.2}MB settled={} ns_per_node={:.2}",
        heap_type, workers, build_ms, query_ms, total_ms, peak_anon_mb, settled_nodes, ns_per_node
    );
}
