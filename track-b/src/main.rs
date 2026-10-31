use std::fs::File;
use std::io::{self, Write};
use std::os::fd::AsRawFd;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

#[repr(C)]
struct Term {
    bytes: Vec<u8>,
    hash: u64,
    df: usize,
    bitmap: bool,
    offset: usize,
}
enum Query {
    Bool(Vec<usize>, Vec<u8>),
    Phrase(Vec<usize>),
}
enum Set {
    List(Vec<u32>),
    Bits(Vec<u64>),
}

fn folded(x: u8) -> u8 {
    if x >= b'A' && x <= b'Z' {
        x + 32
    } else {
        x
    }
}
fn is_word(x: u8) -> bool {
    x >= b'a' && x <= b'z' || x >= b'0' && x <= b'9'
}
fn lookup(terms: &[Term], slots: &[u32], raw: &[u8]) -> Option<usize> {
    if raw.is_empty() {
        return None;
    }
    let mut h = 0xcbf29ce484222325u64;
    for &x in raw {
        let y = folded(x);
        h = (h ^ y as u64).wrapping_mul(0x100000001b3);
    }
    let mut p = h as usize & (slots.len() - 1);
    loop {
        let v = slots[p];
        if v == 0 {
            return None;
        }
        let t = (v - 1) as usize;
        if terms[t].hash == h
            && terms[t].bytes.len() == raw.len()
            && terms[t].bytes.iter().zip(raw).all(|(&a, &b)| a == folded(b))
        {
            return Some(t);
        }
        p = (p + 1) & (slots.len() - 1);
    }
}
fn intern(raw: &[u8], terms: &mut Vec<Term>, slots: &mut Vec<u32>) -> usize {
    let mut h = 0xcbf29ce484222325u64;
    for &x in raw {
        h = (h ^ x as u64).wrapping_mul(0x100000001b3);
    }
    let mut p = h as usize & (slots.len() - 1);
    loop {
        let v = slots[p];
        if v == 0 {
            let id = terms.len();
            terms.push(Term { bytes: raw.to_vec(), hash: h, df: 0, bitmap: false, offset: 0 });
            slots[p] = (id + 1) as u32;
            return id;
        }
        let t = (v - 1) as usize;
        if terms[t].hash == h && terms[t].bytes == raw {
            return t;
        }
        p = (p + 1) & (slots.len() - 1);
    }
}
fn parse_queries(data: &[u8]) -> (Vec<Term>, Vec<Query>) {
    let mut cap = 1usize;
    while cap < data.len() / 4 {
        cap *= 2;
    }
    let mut slots = vec![0u32; cap];
    let mut terms = Vec::new();
    let mut queries = Vec::new();
    for line in data.split(|&b| b == b'\n').filter(|x| !x.is_empty()) {
        let mut p = 0;
        while p < line.len() && line[p] != b' ' {
            p += 1;
        }
        let head = &line[..p];
        if head == b"BOOL" {
            let mut ids = Vec::new();
            let mut ops = Vec::new();
            let mut q = p + 1;
            while q < line.len() {
                let s = q;
                while q < line.len() && line[q] != b' ' {
                    q += 1;
                }
                ids.push(intern(&line[s..q], &mut terms, &mut slots));
                if q < line.len() {
                    q += 1;
                    let op = &line[q..];
                    if op.starts_with(b"OR") {
                        ops.push(1);
                        q += 2;
                    } else if op.starts_with(b"NOT") {
                        ops.push(2);
                        q += 3;
                    } else {
                        ops.push(0);
                        q += 3;
                    }
                    if q < line.len() {
                        q += 1;
                    }
                }
            }
            queries.push(Query::Bool(ids, ops));
        } else {
            let s = p + 2;
            let e = line.len() - 1;
            let mut ids = Vec::new();
            for t in line[s..e].split(|&b| b == b' ') {
                ids.push(intern(t, &mut terms, &mut slots));
            }
            queries.push(Query::Phrase(ids));
        }
    }
    (terms, queries)
}
#[cfg(target_os = "linux")]
fn map_file(path: &Path) -> io::Result<(File, *mut u8, usize)> {
    use std::ffi::c_void;
    unsafe extern "C" {
        fn mmap(addr: *mut c_void, len: usize, prot: i32, flags: i32, fd: i32, off: i64) -> *mut c_void;
    }
    let f = File::open(path)?;
    let n = f.metadata()?.len() as usize;
    // SAFETY: mmap receives a valid open descriptor and file length; the mapping is read-only and kept alive with `f`.
    let p = unsafe { mmap(std::ptr::null_mut(), n, 1, 2, f.as_raw_fd(), 0) };
    if p as isize == -1 {
        return Err(io::Error::last_os_error());
    }
    Ok((f, p.cast(), n))
}
fn merge(a: &[u32], b: &[u32], op: u8) -> Vec<u32> {
    let mut out = Vec::with_capacity(if op == 1 { a.len() + b.len() } else { a.len().min(b.len()) });
    let (mut i, mut j) = (0, 0);
    while i < a.len() && j < b.len() {
        if a[i] == b[j] {
            if op != 2 {
                out.push(a[i]);
            }
            i += 1;
            j += 1;
        } else if a[i] < b[j] {
            if op != 0 {
                out.push(a[i]);
            }
            i += 1;
        } else {
            if op == 1 {
                out.push(b[j]);
            }
            j += 1;
        }
    }
    if op != 0 {
        out.extend_from_slice(&a[i..]);
    }
    if op == 1 {
        out.extend_from_slice(&b[j..]);
    }
    out
}
fn as_list(s: Set, n: usize) -> Vec<u32> {
    match s {
        Set::List(v) => v,
        Set::Bits(b) => {
            let mut out = Vec::new();
            for (wi, &w) in b.iter().enumerate() {
                let mut x = w;
                while x != 0 {
                    let z = x.trailing_zeros() as usize;
                    let id = wi * 64 + z;
                    if id < n {
                        out.push(id as u32);
                    }
                    x &= x - 1;
                }
            }
            out
        }
    }
}
fn bitmap_op(a: Set, b: Set, op: u8, n: usize) -> Set {
    let words = (n + 63) / 64;
    let aa = match &a {
        Set::Bits(x) => Some(x.clone()),
        _ => None,
    };
    let bb = match &b {
        Set::Bits(x) => Some(x.clone()),
        _ => None,
    };
    if aa.is_none() && bb.is_none() {
        return Set::List(merge(&as_list(a, n), &as_list(b, n), op));
    }
    let mut out = vec![0u64; words];
    match (aa, bb) {
        (Some(x), Some(y)) => {
            for i in 0..words {
                out[i] = if op == 0 {
                    x[i] & y[i]
                } else if op == 1 {
                    x[i] | y[i]
                } else {
                    x[i] & !y[i]
                };
            }
        }
        (Some(x), None) => {
            if op == 0 {
                let v = as_list(b, n);
                for id in v {
                    let m = 1u64 << (id % 64);
                    if x[id as usize / 64] & m != 0 {
                        out[id as usize / 64] |= m;
                    }
                }
            } else {
                out.copy_from_slice(&x);
                let v = as_list(b, n);
                for id in v {
                    let m = 1u64 << (id % 64);
                    if op == 1 {
                        out[id as usize / 64] |= m
                    } else {
                        out[id as usize / 64] &= !m;
                    }
                }
            }
        }
        (None, Some(y)) => {
            if op == 1 {
                out.copy_from_slice(&y);
            }
            let v = as_list(a, n);
            for id in v {
                let m = 1u64 << (id % 64);
                if op == 0 {
                    if y[id as usize / 64] & m != 0 {
                        out[id as usize / 64] |= m
                    }
                } else {
                    out[id as usize / 64] |= m;
                }
            }
            if op == 2 {
                for i in 0..words {
                    out[i] &= !y[i];
                }
            }
        }
        _ => unreachable!(),
    }
    if op == 1 {
        let last = n % 64;
        if last != 0 {
            out[words - 1] &= (1u64 << last) - 1;
        }
    }
    let count: usize = out.iter().map(|x| x.count_ones() as usize).sum();
    if count < n / 64 {
        Set::List(as_list(Set::Bits(out), n))
    } else {
        Set::Bits(out)
    }
}
fn term_set(t: usize, terms: &[Term], posts: &[u32], bits: &[u64], n: usize) -> Set {
    let x = &terms[t];
    if x.bitmap {
        let w = (n + 63) / 64;
        Set::Bits(bits[x.offset..x.offset + w].to_vec())
    } else {
        Set::List(posts[x.offset..x.offset + x.df].to_vec())
    }
}
fn output_set(s: Set, n: usize, out: &mut Vec<u8>) {
    let (count, sum, first) = match s {
        Set::List(v) => {
            (v.len(), v.iter().map(|&x| x as u64).sum::<u64>(), v.iter().take(10).copied().collect::<Vec<_>>())
        }
        Set::Bits(b) => {
            let mut count = 0usize;
            let mut sum = 0u64;
            let mut first = Vec::new();
            for (wi, &w) in b.iter().enumerate() {
                count += w.count_ones() as usize;
                let mut x = w;
                while x != 0 {
                    let bit = x.trailing_zeros() as usize;
                    let id = wi * 64 + bit;
                    if id < n {
                        sum += id as u64;
                        if first.len() < 10 {
                            first.push(id as u32);
                        }
                    }
                    x &= x - 1;
                }
            }
            (count, sum, first)
        }
    };
    write!(out, "{count} {sum}").unwrap();
    for id in first {
        write!(out, " {id}").unwrap();
    }
    out.push(b'\n');
}
fn document_id(line: &[u8]) -> (usize, usize) {
    let mut tab = 0;
    let mut id = 0usize;
    while tab < line.len() && line[tab] != b'\t' {
        id = id * 10 + (line[tab] - b'0') as usize;
        tab += 1;
    }
    (id, tab)
}
fn count_chunk(docs: &[u8], start: usize, end: usize, terms: &[Term], table: &[u32]) -> (Vec<u32>, usize) {
    let mut counts = vec![0u32; terms.len()];
    let mut last = vec![u32::MAX; terms.len()];
    let (mut pos, mut rows) = (start, 0usize);
    while pos < end {
        let mut line_end = pos;
        while line_end < end && docs[line_end] != b'\n' {
            line_end += 1;
        }
        let line = &docs[pos..line_end];
        let (id, tab) = document_id(line);
        let id = id as u32;
        let mut p = tab + 1;
        while p < line.len() {
            while p < line.len() && !is_word(folded(line[p])) {
                p += 1;
            }
            let s = p;
            while p < line.len() && is_word(folded(line[p])) {
                p += 1;
            }
            if p > s {
                if let Some(t) = lookup(terms, table, &line[s..p]) {
                    if last[t] != id {
                        last[t] = id;
                        counts[t] += 1;
                    }
                }
            }
        }
        rows += 1;
        pos = line_end + 1;
    }
    (counts, rows)
}
fn fill_chunk(
    docs: &[u8],
    start: usize,
    end: usize,
    chunk: usize,
    terms: &[Term],
    table: &[u32],
    counts: &[Vec<u32>],
    sparse_offsets: &[usize],
    bitmaps: &[AtomicU64],
    post_ptr: usize,
    text_ptr: usize,
    term_count: usize,
) -> Vec<usize> {
    let mut cursor = vec![0usize; term_count];
    let mut last = vec![u32::MAX; term_count];
    let mut pos = start;
    while pos < end {
        let mut line_end = pos;
        while line_end < end && docs[line_end] != b'\n' {
            line_end += 1;
        }
        let line = &docs[pos..line_end];
        let (id, tab) = document_id(line);
        let id = id as u32;
        // SAFETY: input lines have unique dense IDs, and each chunk owns a disjoint contiguous document range.
        unsafe {
            (text_ptr as *mut u32).add(id as usize).write((pos + tab + 1) as u32);
        }
        let mut p = tab + 1;
        while p < line.len() {
            while p < line.len() && !is_word(folded(line[p])) {
                p += 1;
            }
            let s = p;
            while p < line.len() && is_word(folded(line[p])) {
                p += 1;
            }
            if p > s {
                if let Some(t) = lookup(terms, table, &line[s..p]) {
                    if last[t] != id {
                        last[t] = id;
                        let term = &terms[t];
                        if term.bitmap {
                            bitmaps[term.offset + id as usize / 64].fetch_or(1u64 << (id % 64), Ordering::Relaxed);
                        } else {
                            let slot = sparse_offsets[chunk * term_count + t] + cursor[t];
                            // SAFETY: pass A computed exact per-chunk term counts; these prefix intervals are disjoint between all worker threads.
                            unsafe {
                                (post_ptr as *mut u32).add(slot).write(id);
                            }
                            cursor[t] += 1;
                        }
                    }
                }
            }
        }
        pos = line_end + 1;
    }
    debug_assert!(counts[chunk].iter().enumerate().all(|(t, &n)| terms[t].bitmap || cursor[t] == n as usize));
    cursor
}
fn eval_query(
    q: &Query,
    terms: &[Term],
    table: &[u32],
    posts: &[u32],
    bits: &[u64],
    docs: &[u8],
    text_starts: &[u32],
    n: usize,
) -> Vec<u8> {
    let mut out = Vec::with_capacity(64);
    match q {
        Query::Bool(ids, ops) => {
            let mut r = term_set(ids[0], terms, posts, bits, n);
            for (i, &t) in ids.iter().enumerate().skip(1) {
                r = bitmap_op(r, term_set(t, terms, posts, bits, n), ops[i - 1], n);
            }
            output_set(r, n, &mut out);
        }
        Query::Phrase(ids) => {
            let mut anchor = ids[0];
            for &t in ids.iter().skip(1) {
                if terms[t].df < terms[anchor].df {
                    anchor = t;
                }
            }
            let candidates = as_list(term_set(anchor, terms, posts, bits, n), n);
            let mut found = Vec::new();
            for d in candidates {
                let mut tok = Vec::<usize>::new();
                let start = text_starts[d as usize] as usize;
                let end = if (d as usize) + 1 < n { text_starts[d as usize + 1] as usize - 1 } else { docs.len() - 1 };
                let mut p = start;
                while p < end {
                    while p < end && !is_word(folded(docs[p])) {
                        p += 1;
                    }
                    let s = p;
                    while p < end && is_word(folded(docs[p])) {
                        p += 1;
                    }
                    if p > s {
                        tok.push(lookup(terms, table, &docs[s..p]).unwrap_or(usize::MAX));
                    }
                }
                if tok.windows(ids.len()).any(|w| w == ids.as_slice()) {
                    found.push(d);
                }
            }
            let sum: u64 = found.iter().map(|&x| x as u64).sum();
            write!(out, "{} {sum}", found.len()).unwrap();
            for id in found.iter().take(10) {
                write!(out, " {id}").unwrap();
            }
            out.push(b'\n');
        }
    }
    out
}
fn main() -> io::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let root = if args.len() > 1 { args[1].as_str() } else { "/data" };
    let output_dir = args.get(2).map_or("/out", String::as_str);
    let qdata = std::fs::read(format!("{root}/queries.txt"))?;
    let mut t0 = Instant::now();
    let (mut terms, queries) = parse_queries(&qdata);
    eprintln!("parse_queries_ms={}", t0.elapsed().as_millis());
    let (_file, ptr, len) = map_file(Path::new(&format!("{root}/docs.txt")))?;
    let docs = unsafe { std::slice::from_raw_parts(ptr, len) }; // SAFETY: read-only slice covers the live mmap and never outlives the mapping.
    let mut slots = 1usize;
    while slots < terms.len() * 2 {
        slots *= 2;
    }
    let mut table = vec![0u32; slots];
    for (i, t) in terms.iter().enumerate() {
        let mut p = t.hash as usize & (slots - 1);
        while table[p] != 0 {
            p = (p + 1) & (slots - 1);
        }
        table[p] = (i + 1) as u32;
    }
    let scan_workers = std::thread::available_parallelism().map_or(1, usize::from).min(12);
    let mut boundaries = vec![0usize];
    for i in 1..scan_workers {
        let target = len * i / scan_workers;
        let mut start = target;
        while start < len && docs[start - 1] != b'\n' {
            start += 1;
        }
        if start < len && start > *boundaries.last().unwrap() {
            boundaries.push(start);
        }
    }
    boundaries.push(len);
    let chunks: Vec<(usize, usize)> = boundaries.windows(2).map(|w| (w[0], w[1])).collect();
    t0 = Instant::now();
    let pass_a: Vec<(Vec<u32>, usize)> = std::thread::scope(|scope| {
        let mut handles = Vec::new();
        for &(start, end) in &chunks {
            let terms_ref = &terms;
            let table_ref = &table;
            handles.push(scope.spawn(move || count_chunk(docs, start, end, terms_ref, table_ref)));
        }
        handles.into_iter().map(|h| h.join().expect("count worker panicked")).collect()
    });
    let rows: Vec<usize> = pass_a.iter().map(|x| x.1).collect();
    let chunk_counts: Vec<Vec<u32>> = pass_a.into_iter().map(|x| x.0).collect();
    let n: usize = rows.iter().sum();
    eprintln!("pass_a_ms={}", t0.elapsed().as_millis());
    for (ti, term) in terms.iter_mut().enumerate() {
        term.df = chunk_counts.iter().map(|c| c[ti] as usize).sum();
        term.bitmap = term.df >= n / 64.max(1);
    }
    let words = n.div_ceil(64);
    let mut bit_total = 0usize;
    let mut post_total = 0usize;
    for term in &mut terms {
        if term.bitmap {
            term.offset = bit_total;
            bit_total += words;
        } else {
            term.offset = post_total;
            post_total += term.df;
        }
    }
    let mut sparse_offsets = vec![0usize; chunks.len() * terms.len()];
    for (ti, term) in terms.iter().enumerate().filter(|(_, t)| !t.bitmap) {
        let mut offset = term.offset;
        for ci in 0..chunks.len() {
            sparse_offsets[ci * terms.len() + ti] = offset;
            offset += chunk_counts[ci][ti] as usize;
        }
        debug_assert_eq!(offset, term.offset + term.df);
    }
    let bitmaps: Vec<AtomicU64> = (0..bit_total).map(|_| AtomicU64::new(0)).collect();
    let mut posts = vec![0u32; post_total];
    let mut text_starts = vec![0u32; n];
    t0 = Instant::now();
    let post_ptr = posts.as_mut_ptr() as usize;
    let text_ptr = text_starts.as_mut_ptr() as usize;
    std::thread::scope(|scope| {
        let mut handles = Vec::new();
        for (ci, &(start, end)) in chunks.iter().enumerate() {
            let terms_ref = &terms;
            let table_ref = &table;
            let counts_ref = &chunk_counts;
            let offsets_ref = &sparse_offsets;
            let bitmaps_ref = &bitmaps;
            handles.push(scope.spawn(move || {
                fill_chunk(
                    docs,
                    start,
                    end,
                    ci,
                    terms_ref,
                    table_ref,
                    counts_ref,
                    offsets_ref,
                    bitmaps_ref,
                    post_ptr,
                    text_ptr,
                    terms_ref.len(),
                )
            }));
        }
        for handle in handles {
            handle.join().expect("postings worker panicked");
        }
    });
    let bits: Vec<u64> = bitmaps.iter().map(|x| x.load(Ordering::Relaxed)).collect();
    eprintln!("pass_b_ms={}", t0.elapsed().as_millis());
    t0 = Instant::now();
    let workers = std::thread::available_parallelism().map_or(1, usize::from).min(12).min(queries.len().max(1));
    let block = queries.len().div_ceil(workers);
    let mut out = Vec::with_capacity(queries.len() * 24);
    std::thread::scope(|scope| {
        let mut handles = Vec::new();
        for chunk in queries.chunks(block) {
            let terms_ref = &terms;
            let table_ref = &table;
            let posts_ref = &posts;
            let bits_ref = &bits;
            let starts_ref = &text_starts;
            handles.push(scope.spawn(move || {
                let mut part = Vec::new();
                for q in chunk {
                    part.extend_from_slice(&eval_query(
                        q, terms_ref, table_ref, posts_ref, bits_ref, docs, starts_ref, n,
                    ));
                }
                part
            }));
        }
        for h in handles {
            out.extend(h.join().expect("query worker panicked"));
        }
    });
    eprintln!("query_phase_ms={}", t0.elapsed().as_millis());
    let t0 = Instant::now();
    std::fs::create_dir_all(output_dir)?;
    std::fs::write(Path::new(output_dir).join("results.txt"), &out)?;
    let mut stdout = io::BufWriter::new(io::stdout().lock());
    stdout.write_all(&out)?;
    stdout.flush()?;
    eprintln!("output_ms={}", t0.elapsed().as_millis());
    Ok(())
}
