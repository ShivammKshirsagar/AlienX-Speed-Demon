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

// SAFETY: Mapping owns the mmap allocation and raw pointer, safe to send and share.
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
        // SAFETY: MAP_PRIVATE (2) with PROT_READ (1). Valid fd and length.
        let ptr = unsafe { mmap(std::ptr::null_mut(), len, 1, 2, file.as_raw_fd(), 0) };
        assert_ne!(ptr as isize, -1, "mmap failed");
        Self { ptr, len }
    }

    #[inline(always)]
    fn bytes(&self) -> &[u8] {
        if self.len == 0 {
            &[]
        } else {
            // SAFETY: self.ptr points to self.len valid readable bytes mapped from the input file.
            unsafe { std::slice::from_raw_parts(self.ptr, self.len) }
        }
    }
}

impl Drop for Mapping {
    fn drop(&mut self) {
        if self.len != 0 {
            // SAFETY: self.ptr was returned by mmap and is unmapped exactly once upon drop.
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
            // SAFETY: i is masked by self.mask < self.slots.len().
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
            // SAFETY: i is masked by self.mask < self.slots.len().
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
            // SAFETY: idx in self.touched is a valid slot index populated in try_relax/grow.
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
    // SAFETY: Cap concurrent fallbacks to 1 so at most 1 temporary 14 MB distance vector exists.
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
            // SAFETY: i is within valid edge range.
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

fn main() {
    unsafe {
        mallopt(-1, 65536); // M_TRIM_THRESHOLD
        mallopt(-3, 65536); // M_MMAP_THRESHOLD
        mallopt(-8, 2); // M_ARENA_MAX
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
        while p < edges_data.len() && edges_data[p - 1] != b'\n' {
            p += 1;
        }
        bounds.push(p);
    }
    bounds.push(edges_data.len());

    let degrees = (0..n).map(|_| AtomicU32::new(0)).collect::<Vec<_>>();

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
                    // SAFETY: cur is verified < slice.len() and points to digit.
                    let u = unsafe { parse_u32_raw(ptr, &mut cur) };
                    // SAFETY: u < n as guaranteed by graph specification.
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
                    // SAFETY: cur is bounds-checked and points to digit.
                    let u = unsafe { parse_u32_raw(ptr, &mut cur) };
                    cur += 1;
                    let v = unsafe { parse_u32_raw(ptr, &mut cur) };
                    cur += 1;
                    let w = unsafe { parse_u32_raw(ptr, &mut cur) };
                    while cur < slice.len() && (slice[cur] == b'\r' || slice[cur] == b'\n') {
                        cur += 1;
                    }
                    // SAFETY: u < n guaranteed by spec; write_pos partition allocates unique slot < sum.
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
    unsafe {
        malloc_trim(0);
    }

    let mut has_in = vec![false; n as usize];
    for e in &edges {
        has_in[e.0 as usize] = true;
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
        // SAFETY: q_pos < q_data.len() and points to digit.
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
                    let mut heap = QuadHeap::new();

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
                            if u == t {
                                ans = d;
                                break;
                            }
                            let start = offsets[u as usize] as usize;
                            let end = offsets[u as usize + 1] as usize;
                            for i in start..end {
                                // SAFETY: i is within valid edge range.
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
