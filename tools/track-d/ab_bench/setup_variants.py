import os

# Read base step4.rs
with open('tools/track-d/ab_bench/step4.rs', 'r', encoding='utf-8') as f:
    s4 = f.read().replace('\r\n', '\n')

# Also read step3.rs if present
if os.path.exists('tools/track-d/ab_bench/step3.rs'):
    with open('tools/track-d/ab_bench/step3.rs', 'r', encoding='utf-8') as f:
        s3 = f.read().replace('\r\n', '\n')
    s3 = s3.replace(
        'let num_threads = thread::available_parallelism()\n        .map_or(1, usize::from)\n        .min(12)\n        .min(m.max(1) as usize);',
        'let num_threads = std::env::var("WORKERS").ok().and_then(|s| s.parse().ok()).unwrap_or(12).min(m.max(1) as usize);'
    )
    s3 = s3.replace(
        'let q_workers = thread::available_parallelism()\n            .map_or(1, usize::from)\n            .min(12)\n            .min(to_solve.len());',
        'let q_workers = std::env::var("WORKERS").ok().and_then(|s| s.parse().ok()).unwrap_or(12).min(to_solve.len());'
    )
    s3 = s3.replace('fn main() {', 'fn main() {\n    let t_total_start = std::time::Instant::now();')
    s3 = s3.replace('    if !to_solve.is_empty() {', '    let t_query_start = std::time::Instant::now();\n    if !to_solve.is_empty() {')
    s3 = s3.replace(
        '    let mut out_buf = Vec::with_capacity(num_queries * 12);',
        '''    let query_ms = t_query_start.elapsed().as_secs_f64() * 1000.0;
    let total_ms = t_total_start.elapsed().as_secs_f64() * 1000.0;
    eprintln!("TIMING: query={:.2}ms total={:.2}ms", query_ms, total_ms);

    let mut out_buf = Vec::with_capacity(num_queries * 12);'''
    )
    os.makedirs('tools/track-d/ab_bench/src/bin', exist_ok=True)
    with open('tools/track-d/ab_bench/src/bin/step3.rs', 'w', encoding='utf-8') as f:
        f.write(s3)

s4 = s4.replace(
    'let num_threads = thread::available_parallelism()\n        .map_or(1, usize::from)\n        .min(12)\n        .min(m.max(1) as usize);',
    'let num_threads = std::env::var("WORKERS").ok().and_then(|s| s.parse().ok()).unwrap_or(12).min(m.max(1) as usize);'
)
s4 = s4.replace(
    'let q_workers = thread::available_parallelism()\n            .map_or(1, usize::from)\n            .min(12)\n            .min(to_solve.len());',
    'let q_workers = std::env::var("WORKERS").ok().and_then(|s| s.parse().ok()).unwrap_or(12).min(to_solve.len());'
)
s4 = s4.replace('fn main() {', 'fn main() {\n    let t_total_start = std::time::Instant::now();')
s4 = s4.replace('    if !to_solve.is_empty() {', '    let t_query_start = std::time::Instant::now();\n    if !to_solve.is_empty() {')
s4 = s4.replace(
    '    let mut out_buf = Vec::with_capacity(num_queries * 12);',
    '''    let query_ms = t_query_start.elapsed().as_secs_f64() * 1000.0;
    let total_ms = t_total_start.elapsed().as_secs_f64() * 1000.0;
    eprintln!("TIMING: query={:.2}ms total={:.2}ms", query_ms, total_ms);

    let mut out_buf = Vec::with_capacity(num_queries * 12);'''
)

os.makedirs('tools/track-d/ab_bench/src/bin', exist_ok=True)

# 1. step4_current
with open('tools/track-d/ab_bench/src/bin/step4_current.rs', 'w', encoding='utf-8') as f:
    f.write(s4)

# 2. step4_no_prefetch
prefetch_block = '''                                        heap.push(nd, e.0);
                                        let v_off = unsafe { *offsets.get_unchecked(e.0 as usize) as usize };
                                        unsafe { std::arch::x86_64::_mm_prefetch(edges.as_ptr().add(v_off) as *const i8, std::arch::x86_64::_MM_HINT_T0); }'''
no_prefetch_block = '''                                        heap.push(nd, e.0);'''

assert prefetch_block in s4, "prefetch_block not found in s4"
s4_no_prefetch = s4.replace(prefetch_block, no_prefetch_block)
with open('tools/track-d/ab_bench/src/bin/step4_no_prefetch.rs', 'w', encoding='utf-8') as f:
    f.write(s4_no_prefetch)

# 3. step4_prefetch_offset
prefetch_offset_block = '''                                        heap.push(nd, e.0);
                                        unsafe { std::arch::x86_64::_mm_prefetch(offsets.as_ptr().add(e.0 as usize) as *const i8, std::arch::x86_64::_MM_HINT_T0); }'''
s4_prefetch_offset = s4.replace(prefetch_block, prefetch_offset_block)
with open('tools/track-d/ab_bench/src/bin/step4_prefetch_offset.rs', 'w', encoding='utf-8') as f:
    f.write(s4_prefetch_offset)

# 4. step4_pop_prefetch (offset prefetch at push + pop next adjacency prefetch)
pop_prefetch_target = '                        while let Some((d, u)) = heap.pop() {\n                            let cur = compact.get(u);'
pop_prefetch_replacement = '''                        while let Some((d, u)) = heap.pop() {
                            if let Some(&HeapItem(_, next_u)) = heap.0.first() {
                                let next_off = unsafe { *offsets.get_unchecked(next_u as usize) as usize };
                                unsafe { std::arch::x86_64::_mm_prefetch(edges.as_ptr().add(next_off) as *const i8, std::arch::x86_64::_MM_HINT_T0); }
                            }
                            let cur = compact.get(u);'''
s4_pop_prefetch = s4_prefetch_offset.replace(pop_prefetch_target, pop_prefetch_replacement)
with open('tools/track-d/ab_bench/src/bin/step4_pop_prefetch.rs', 'w', encoding='utf-8') as f:
    f.write(s4_pop_prefetch)

# 5. step4_no_shrink (large INIT_CAP, no trim, no buffer shrinking)
compact_shrink_target = '''    #[inline(always)]
    fn clear_and_shrink(&mut self) {
        for &idx in &self.touched {
            // SAFETY: idx in self.touched is a valid slot index populated in try_relax/grow.
            unsafe { *self.slots.get_unchecked_mut(idx) = Slot(EMPTY_SLOT, INF32); }
        }
        self.touched.clear();
        if self.slots.len() >= SHRINK_THRESH {
            self.slots = vec![Slot(EMPTY_SLOT, INF32); INIT_CAP];
            self.mask = INIT_CAP - 1;
            self.touched.shrink_to(1024);
        }
    }'''

compact_no_shrink = '''    #[inline(always)]
    fn clear_and_shrink(&mut self) {
        for &idx in &self.touched {
            // SAFETY: idx in self.touched is a valid slot index populated in try_relax/grow.
            unsafe { *self.slots.get_unchecked_mut(idx) = Slot(EMPTY_SLOT, INF32); }
        }
        self.touched.clear();
    }'''

heap_shrink_target = '''    #[inline(always)]
    fn clear_and_shrink(&mut self) {
        self.0.clear();
        if self.0.capacity() > 4096 { self.0.shrink_to(1024); }
    }'''

heap_no_shrink = '''    #[inline(always)]
    fn clear_and_shrink(&mut self) {
        self.0.clear();
    }'''

s4_no_shrink = s4.replace('const INIT_CAP: usize = 16384;', 'const INIT_CAP: usize = 65536;')
s4_no_shrink = s4_no_shrink.replace(compact_shrink_target, compact_no_shrink)
s4_no_shrink = s4_no_shrink.replace(heap_shrink_target, heap_no_shrink)
s4_no_shrink = s4_no_shrink.replace('    unsafe { malloc_trim(0); }\n', '    // no trim\n')
with open('tools/track-d/ab_bench/src/bin/step4_no_shrink.rs', 'w', encoding='utf-8') as f:
    f.write(s4_no_shrink)

# 6. step4_no_fallback_lock
fallback_lock_target = '''    // SAFETY: Cap concurrent fallbacks to 1 so at most 1 temporary 14 MB distance vector exists.
    while FALLBACK_LOCK.swap(true, Ordering::Acquire) {
        std::hint::spin_loop();
    }'''
fallback_unlock_target = '''    FALLBACK_LOCK.store(false, Ordering::Release);'''

s4_no_fallback_lock = s4.replace(fallback_lock_target, '    // No fallback lock')
s4_no_fallback_lock = s4_no_fallback_lock.replace(fallback_unlock_target, '    // No fallback lock')
with open('tools/track-d/ab_bench/src/bin/step4_no_fallback_lock.rs', 'w', encoding='utf-8') as f:
    f.write(s4_no_fallback_lock)

print('setup_variants.py: all benchmark variants generated successfully')
