import subprocess
import os

with open("tools/track-d/candidates/packed_only.rs", "r", encoding="utf-8") as f:
    packed_src = f.read()

# For combined, replace the worker loop in packed_src with SearchState
interleaved_packed_code = '''#[derive(Copy, Clone)]
enum StepResult {
    Continue,
    Done(u64),
    NeedFallback,
}

struct SearchState {
    compact: CompactDist,
    heap: QuadHeap,
    s: u32,
    t: u32,
    query_idx: usize,
    active: bool,
}

impl SearchState {
    fn new() -> Self {
        Self {
            compact: CompactDist::new(),
            heap: QuadHeap::new(),
            s: 0,
            t: 0,
            query_idx: 0,
            active: false,
        }
    }

    #[inline(always)]
    fn init(&mut self, s: u32, t: u32, query_idx: usize) {
        self.s = s;
        self.t = t;
        self.query_idx = query_idx;
        self.active = true;
        self.compact.try_relax(s, 0);
        self.heap.push(0, s);
    }

    #[inline(always)]
    fn clear(&mut self) {
        self.compact.clear_and_shrink();
        self.heap.clear_and_shrink();
    }

    #[inline(always)]
    fn step(
        &mut self,
        offsets: &[u32],
        packed_edges: &[u32],
        w_bits: u32,
        w_mask: u32,
        escape_w: u32,
    ) -> StepResult {
        loop {
            let (d, u) = match self.heap.pop() {
                Some(item) => item,
                None => return StepResult::Done(INF),
            };
            if let Some(&HeapItem(_, next_u)) = self.heap.0.first() {
                let next_off = unsafe { *offsets.get_unchecked(next_u as usize) as usize };
                unsafe {
                    std::arch::x86_64::_mm_prefetch(
                        packed_edges.as_ptr().add(next_off) as *const i8,
                        std::arch::x86_64::_MM_HINT_T0,
                    );
                }
            }
            let cur = self.compact.get(u);
            if cur == INF32 || d > cur as u64 {
                continue;
            }
            if u == self.t {
                return StepResult::Done(d);
            }
            let start = offsets[u as usize] as usize;
            let end = offsets[u as usize + 1] as usize;
            let mut i = start;
            while i < end {
                let p = unsafe { *packed_edges.get_unchecked(i) };
                let v = p >> w_bits;
                let wf = p & w_mask;
                let (w, step) = if wf != escape_w {
                    (wf as u64, 1)
                } else {
                    (unsafe { *packed_edges.get_unchecked(i + 1) as u64 }, 2)
                };
                i += step;
                let nd = d + w;
                if nd >= INF32 as u64 {
                    return StepResult::NeedFallback;
                }
                match self.compact.try_relax(v, nd as u32) {
                    Some(true) => {
                        self.heap.push(nd, v);
                        unsafe {
                            std::arch::x86_64::_mm_prefetch(
                                offsets.as_ptr().add(v as usize) as *const i8,
                                std::arch::x86_64::_MM_HINT_T0,
                            );
                        }
                    }
                    Some(false) => {}
                    None => return StepResult::NeedFallback,
                }
            }
            return StepResult::Continue;
        }
    }
}
'''

combined_src = packed_src.replace("fn main() {", interleaved_packed_code + "\nfn main() {")

packed_worker_loop = '''                scope.spawn(move || {
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
                                        packed_edges.as_ptr().add(next_off) as *const i8,
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
                            let mut i = start;
                            while i < end {
                                let p = unsafe { *packed_edges.get_unchecked(i) };
                                let v = p >> w_bits;
                                let wf = p & w_mask;
                                let (w, step) = if wf != escape_w {
                                    (wf as u64, 1)
                                } else {
                                    (unsafe { *packed_edges.get_unchecked(i + 1) as u64 }, 2)
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
                                s,
                                t,
                                n,
                                offsets,
                                packed_edges,
                                w_bits,
                                w_mask,
                                escape_w,
                                &mut heap,
                            );
                        }

                        results[idx].store(ans, Ordering::Relaxed);
                    }
                });'''

combined_worker_loop = '''                scope.spawn(move || {
                    let mut states = [SearchState::new(), SearchState::new()];
                    for st in states.iter_mut() {
                        let widx = work_idx.fetch_add(1, Ordering::Relaxed);
                        if widx < to_solve.len() {
                            let idx = to_solve[widx];
                            let (s, t) = queries[idx];
                            st.init(s, t, idx);
                        }
                    }
                    let mut active_count = states.iter().filter(|s| s.active).count();
                    let mut cur = 0;
                    while active_count > 0 {
                        let st = &mut states[cur];
                        if st.active {
                            match st.step(offsets, packed_edges, w_bits, w_mask, escape_w) {
                                StepResult::Continue => {}
                                StepResult::Done(ans) => {
                                    results[st.query_idx].store(ans, Ordering::Relaxed);
                                    st.clear();
                                    let widx = work_idx.fetch_add(1, Ordering::Relaxed);
                                    if widx < to_solve.len() {
                                        let idx = to_solve[widx];
                                        let (s, t) = queries[idx];
                                        st.init(s, t, idx);
                                    } else {
                                        st.active = false;
                                        active_count -= 1;
                                    }
                                }
                                StepResult::NeedFallback => {
                                    let ans = fallback_dijkstra(st.s, st.t, n, offsets, packed_edges, w_bits, w_mask, escape_w, &mut st.heap);
                                    results[st.query_idx].store(ans, Ordering::Relaxed);
                                    st.clear();
                                    let widx = work_idx.fetch_add(1, Ordering::Relaxed);
                                    if widx < to_solve.len() {
                                        let idx = to_solve[widx];
                                        let (s, t) = queries[idx];
                                        st.init(s, t, idx);
                                    } else {
                                        st.active = false;
                                        active_count -= 1;
                                    }
                                }
                            }
                        }
                        cur = (cur + 1) % 2;
                    }
                });'''

assert packed_worker_loop in combined_src, "packed_worker_loop not found"
combined_src = combined_src.replace(packed_worker_loop, combined_worker_loop)

with open("tools/track-d/candidates/combined.rs", "w", encoding="utf-8") as f:
    f.write(combined_src)

subprocess.run(["rustfmt", "--edition", "2021", "tools/track-d/candidates/combined.rs"], check=True)
res3 = subprocess.run(["scc", "tools/track-d/candidates/combined.rs"], capture_output=True, text=True)
print("=== Combined Line Count ===")
print(res3.stdout)
