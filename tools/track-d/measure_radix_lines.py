import subprocess
import os

main_code = open("track-d/src/main.rs").read()

radix_heap_code = """
struct RadixHeap {
    buckets: [Vec<(u32, u32)>; 33],
    last: u32,
    size: usize,
    non_empty: u64,
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
    fn push(&mut self, d: u32, u: u32) {
        let x = d ^ self.last;
        let b = if x == 0 { 0 } else { 32 - x.leading_zeros() as usize };
        self.buckets[b].push((d, u));
        self.non_empty |= 1u64 << b;
        self.size += 1;
    }

    #[inline(always)]
    fn pop(&mut self) -> Option<(u32, u32)> {
        if self.size == 0 {
            return None;
        }
        if (self.non_empty & 1) != 0 {
            let res = self.buckets[0].pop().unwrap();
            if self.buckets[0].is_empty() {
                self.non_empty &= !1u64;
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
        self.non_empty &= !(1u64 << b);

        let mut to_redistribute = std::mem::take(&mut self.buckets[b]);
        for (d, u) in to_redistribute.drain(..) {
            let x = d ^ self.last;
            let new_b = if x == 0 { 0 } else { 32 - x.leading_zeros() as usize };
            self.buckets[new_b].push((d, u));
            self.non_empty |= 1u64 << new_b;
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
"""

# Replace QuadHeap in query loop with RadixHeap
# keeping QuadHeap for fallback
combined = main_code.replace("struct QuadHeap(Vec<HeapItem>);", radix_heap_code + "\nstruct QuadHeap(Vec<HeapItem>);")

target_loop = """                    let mut compact = CompactDist::new();
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
                        heap.clear_and_shrink();"""

replacement_loop = """                    let mut compact = CompactDist::new();
                    let mut heap = RadixHeap::new();
                    let mut fb_heap = QuadHeap::new();

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
                            if cur == INF32 || d > cur {
                                continue;
                            }
                            if u == t {
                                ans = d as u64;
                                break;
                            }
                            let mut i = offsets[u as usize] as usize;
                            let end = offsets[u as usize + 1] as usize;
                            while i < end {
                                let p = unsafe { *edges.get_unchecked(i) };
                                let v = p >> w_bits;
                                let wf = p & w_mask;
                                let (w, step) = if wf != escape_w {
                                    (wf, 1)
                                } else {
                                    (unsafe { *edges.get_unchecked(i + 1) }, 2)
                                };
                                i += step;
                                let nd = (d as u64) + (w as u64);
                                if nd >= INF32 as u64 {
                                    need_fallback = true;
                                    break;
                                }
                                let nd32 = nd as u32;
                                match compact.try_relax(v, nd32) {
                                    Some(true) => {
                                        heap.push(nd32, v);
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
                        heap.clear_and_shrink();"""

combined = combined.replace(target_loop, replacement_loop)
combined = combined.replace("&mut heap,", "&mut fb_heap,")

os.makedirs("tools/track-d/radix_candidate", exist_ok=True)
with open("tools/track-d/radix_candidate/main.rs", "w") as f:
    f.write(combined)

subprocess.run(["rustfmt", "--edition", "2021", "tools/track-d/radix_candidate/main.rs"])
res = subprocess.run(["scc", "tools/track-d/radix_candidate/main.rs"], capture_output=True, text=True)
print(res.stdout)
