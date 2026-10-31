import subprocess
import os

main_rs = open("track-d/src/main.rs").read()

# Current QuadHeap definition to replace:
quad_heap_def = """#[derive(Copy, Clone)]
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
}"""

# Line-trimmed RadixHeap that replaces QuadHeap:
radix_heap_def = """struct RadixHeap {
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
}"""

assert quad_heap_def in main_rs, "QuadHeap def not found in main.rs"
cand = main_rs.replace(quad_heap_def, radix_heap_def)

# In fallback_dijkstra: replace QuadHeap with RadixHeap
cand = cand.replace("heap: &mut QuadHeap,", "heap: &mut RadixHeap,")

# In query loop: replace QuadHeap::new() with RadixHeap::new()
cand = cand.replace("let mut heap = QuadHeap::new();", "let mut heap = RadixHeap::new();")

# Replace prefetch:
old_prefetch = """                            if let Some(&HeapItem(_, next_u)) = heap.0.first() {
                                let next_off =
                                    unsafe { *offsets.get_unchecked(next_u as usize) as usize };
                                unsafe {
                                    std::arch::x86_64::_mm_prefetch(
                                        edges.as_ptr().add(next_off) as *const i8,
                                        std::arch::x86_64::_MM_HINT_T0,
                                    );
                                }
                            }"""

new_prefetch = """                            if let Some(&(_, next_u)) = heap.buckets[0].last() {
                                let next_off =
                                    unsafe { *offsets.get_unchecked(next_u as usize) as usize };
                                unsafe {
                                    std::arch::x86_64::_mm_prefetch(
                                        edges.as_ptr().add(next_off) as *const i8,
                                        std::arch::x86_64::_MM_HINT_T0,
                                    );
                                }
                            }"""

assert old_prefetch in cand, "Old prefetch not found in cand"
cand = cand.replace(old_prefetch, new_prefetch)

os.makedirs("tools/track-d/candidates", exist_ok=True)
out_file = "tools/track-d/candidates/radix_everywhere.rs"
with open(out_file, "w") as f:
    f.write(cand)

subprocess.run(["rustfmt", "--edition", "2021", out_file])

# Measure with scc
res_cand = subprocess.run(["scc", out_file], capture_output=True, text=True)
res_orig = subprocess.run(["scc", "track-d/src/main.rs"], capture_output=True, text=True)

print("=== SCC COMPARISON ===")
print("Candidate:")
print(res_cand.stdout)
print("Original:")
print(res_orig.stdout)
