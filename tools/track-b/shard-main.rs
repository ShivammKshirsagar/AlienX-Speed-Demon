#![allow(dead_code)]

#[path = "baseline-src/main.rs"]
mod base;

use std::{
    borrow::Cow,
    ffi::c_void,
    fs::File,
    io,
    os::fd::AsRawFd,
    path::Path,
    sync::{mpsc, Barrier},
    time::Instant,
};

const LIST: u8 = 0;
const BITMAP: u8 = 1;
const QUERY_BLOCK: usize = 1024;

#[derive(Clone, Copy, Default)]
#[repr(C)]
struct Rep {
    offset: u32,
    df: u16,
    kind: u8,
}

struct Plan {
    start: usize,
    end: usize,
    ids: Vec<u32>,
    text: Vec<(u32, u32)>,
    df: Vec<u16>,
    elapsed_us: u128,
}

struct Shard {
    start: usize,
    end: usize,
    ids: Vec<u32>,
    text: Vec<(u32, u32)>,
    reps: Vec<Rep>,
    lists: Vec<u16>,
    bits: Vec<u64>,
    words: usize,
    count_us: u128,
    fill_us: u128,
}

enum Set<'a> {
    List(Cow<'a, [u16]>),
    Bits(Cow<'a, [u64]>),
}

#[derive(Clone, Copy, Default)]
struct QueryResult {
    count: u64,
    sum: u64,
    first: [u32; 10],
    len: usize,
}

#[repr(C)]
struct RUsage([i64; 18]);

unsafe extern "C" {
    fn mmap(addr: *mut c_void, len: usize, prot: i32, flags: i32, fd: i32, off: i64) -> *mut c_void;
    fn madvise(addr: *mut c_void, len: usize, advice: i32) -> i32;
    fn getrusage(who: i32, usage: *mut RUsage) -> i32;
}

fn map_file(path: &Path) -> io::Result<(File, *const u8, usize)> {
    let file = File::open(path)?;
    let len = file.metadata()?.len() as usize;
    // SAFETY: the open file descriptor and checked file length describe the read-only mapping retained with `file`.
    let ptr = unsafe { mmap(std::ptr::null_mut(), len, 1, 2, file.as_raw_fd(), 0) };
    if ptr as isize == -1 { return Err(io::Error::last_os_error()); }
    Ok((file, ptr.cast::<u8>().cast_const(), len))
}

fn advise(ptr: usize, len: usize) {
    // SAFETY: the address and length describe the live read-only mapping; MADV_WILLNEED does not modify it.
    unsafe { let _ = madvise(ptr as *mut c_void, len, 3); }
}

fn proc_mem() -> (u64, u64, u64) {
    let text = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
    let get = |name: &str| text.lines().find_map(|line| line.strip_prefix(name)?.split_whitespace().next()?.parse().ok()).unwrap_or(0);
    (get("RssAnon:"), get("VmHWM:"), get("RssFile:"))
}

fn faults() -> (i64, i64) {
    let mut usage = RUsage([0; 18]);
    // SAFETY: RUsage matches the Linux x86-64 rusage layout and the pointer is writable for the full call.
    assert_eq!(unsafe { getrusage(0, &mut usage) }, 0);
    (usage.0[8], usage.0[9])
}

fn thread_cpu_ns() -> u64 {
    let mut usage = RUsage([0; 18]);
    // SAFETY: RUsage matches the Linux x86-64 rusage layout and the pointer is writable for the full call.
    assert_eq!(unsafe { getrusage(1, &mut usage) }, 0);
    ((usage.0[0] + usage.0[2]) * 1_000_000_000 + (usage.0[1] + usage.0[3]) * 1_000) as u64
}

fn document_id(line: &[u8]) -> (u32, usize) {
    let tab = line.iter().position(|&b| b == b'\t').unwrap_or(line.len());
    let id = line[..tab].iter().fold(0u32, |n, &b| n * 10 + (b - b'0') as u32);
    (id, tab)
}

fn doc_count(docs: &[u8]) -> usize {
    docs.iter().filter(|&&b| b == b'\n').count() + usize::from(!docs.is_empty() && docs.last() != Some(&b'\n'))
}

fn ranges(docs: &[u8], want: usize) -> Vec<(usize, usize)> {
    if docs.is_empty() { return Vec::new(); }
    let mut boundaries = vec![0usize];
    for i in 1..want {
        let mut at = docs.len() * i / want;
        while at < docs.len() && at > 0 && docs[at - 1] != b'\n' { at += 1; }
        if at < docs.len() && at > *boundaries.last().unwrap() { boundaries.push(at); }
    }
    boundaries.push(docs.len());
    boundaries.windows(2).map(|w| (w[0], w[1])).collect()
}

fn count_shard(docs: &[u8], (start, end): (usize, usize), terms: &[base::Term], table: &base::ScanTable) -> Plan {
    let begin = Instant::now();
    let mut ids = Vec::new();
    let mut text = Vec::new();
    let mut df = vec![0u16; terms.len()];
    let mut last = vec![u16::MAX; terms.len()];
    let (mut starts, mut ends) = (Vec::new(), Vec::new());
    let mut pos = start;
    while pos < end {
        let line_end = docs[pos..end].iter().position(|&b| b == b'\n').map_or(end, |x| pos + x);
        let line = &docs[pos..line_end];
        if !line.is_empty() {
            let (id, tab) = document_id(line);
            let local = ids.len();
            assert!(local < u16::MAX as usize, "shard exceeds u16 local ID range");
            ids.push(id);
            text.push(((pos + tab + 1) as u32, line_end as u32));
            let body = &line[tab.saturating_add(1)..];
            base::token_positions(body, &mut starts, &mut ends);
            for (&a, &b) in starts.iter().zip(&ends) {
                if let Some(t) = base::lookup(terms, table, &body[a as usize..b as usize]) {
                    if last[t] != local as u16 { last[t] = local as u16; df[t] += 1; }
                }
            }
        }
        pos = line_end.saturating_add(1);
    }
    Plan { start, end, ids, text, df, elapsed_us: begin.elapsed().as_micros() }
}

fn allocate_shard(plan: Plan, terms: &[base::Term]) -> Shard {
    let Plan { start, end, ids, text, df, elapsed_us } = plan;
    let n = ids.len();
    let words = n.div_ceil(64);
    let mut reps = vec![Rep::default(); terms.len()];
    let mut list_len = 0usize;
    let mut bit_len = 0usize;
    for (t, &df) in df.iter().enumerate() {
        let list_bytes = df as usize * std::mem::size_of::<u16>();
        let bitmap_bytes = words * std::mem::size_of::<u64>();
        if df != 0 && bitmap_bytes < list_bytes {
            reps[t] = Rep { offset: bit_len as u32, df, kind: BITMAP };
            bit_len += words;
        } else {
            reps[t] = Rep { offset: list_len as u32, df, kind: LIST };
            list_len += df as usize;
        }
    }
    Shard { start, end, ids, text, reps, lists: vec![0; list_len], bits: vec![0; bit_len], words, count_us: elapsed_us, fill_us: 0 }
}

fn fill_shard(docs: &[u8], shard: &mut Shard, terms: &[base::Term], table: &base::ScanTable) -> u128 {
    let begin = Instant::now();
    let mut last = vec![u16::MAX; terms.len()];
    let mut cursor = vec![0u16; terms.len()];
    let (mut starts, mut ends) = (Vec::new(), Vec::new());
    let mut local = 0usize;
    let mut pos = shard.start;
    while pos < shard.end {
        let line_end = docs[pos..shard.end].iter().position(|&b| b == b'\n').map_or(shard.end, |x| pos + x);
        let line = &docs[pos..line_end];
        if !line.is_empty() {
            let (_, tab) = document_id(line);
            let body = &line[tab + 1..];
            base::token_positions(body, &mut starts, &mut ends);
            for (&a, &b) in starts.iter().zip(&ends) {
                let raw = &body[a as usize..b as usize];
                if let Some(t) = base::lookup(terms, table, raw) {
                    let rep = shard.reps[t];
                    if last[t] != local as u16 {
                        last[t] = local as u16;
                        if rep.kind == BITMAP {
                            let at = rep.offset as usize + local / 64;
                            shard.bits[at] |= 1u64 << (local % 64);
                        } else {
                            let at = rep.offset as usize + cursor[t] as usize;
                            shard.lists[at] = local as u16;
                            cursor[t] += 1;
                        }
                    }
                }
            }
            local += 1;
        }
        pos = line_end.saturating_add(1);
    }
    debug_assert_eq!(local, shard.ids.len());
    debug_assert!(shard.reps.iter().enumerate().all(|(t, r)| r.kind == BITMAP || cursor[t] == r.df));
    begin.elapsed().as_micros()
}

fn term_set<'a>(shard: &'a Shard, term: usize) -> Set<'a> {
    let rep = shard.reps[term];
    if rep.kind == BITMAP {
        let start = rep.offset as usize;
        Set::Bits(Cow::Borrowed(&shard.bits[start..start + shard.words]))
    } else {
        let start = rep.offset as usize;
        Set::List(Cow::Borrowed(&shard.lists[start..start + rep.df as usize]))
    }
}

fn merge_lists(a: &[u16], b: &[u16], op: u8) -> Vec<u16> {
    let mut out = Vec::with_capacity(if op == 1 { a.len() + b.len() } else { a.len().min(b.len()) });
    let (mut i, mut j) = (0, 0);
    while i < a.len() && j < b.len() {
        if a[i] == b[j] { if op != 2 { out.push(a[i]); } i += 1; j += 1; }
        else if a[i] < b[j] { if op != 0 { out.push(a[i]); } i += 1; }
        else { if op == 1 { out.push(b[j]); } j += 1; }
    }
    if op != 0 { out.extend_from_slice(&a[i..]); }
    if op == 1 { out.extend_from_slice(&b[j..]); }
    out
}

fn merge_lists_into(a: &[u16], b: &[u16], op: u8, out: &mut Vec<u16>) {
    out.clear();
    let (mut i, mut j) = (0, 0);
    while i < a.len() && j < b.len() {
        if a[i] == b[j] { if op != 2 { out.push(a[i]); } i += 1; j += 1; }
        else if a[i] < b[j] { if op != 0 { out.push(a[i]); } i += 1; }
        else { if op == 1 { out.push(b[j]); } j += 1; }
    }
    if op != 0 { out.extend_from_slice(&a[i..]); }
    if op == 1 { out.extend_from_slice(&b[j..]); }
}

fn as_vec(set: Set<'_>, n: usize) -> Vec<u16> {
    match set {
        Set::List(xs) => xs.into_owned(),
        Set::Bits(bits) => {
            let mut out = Vec::new();
            for (w, &word) in bits.iter().enumerate() {
                let mut x = word;
                while x != 0 {
                    let local = w * 64 + x.trailing_zeros() as usize;
                    if local < n { out.push(local as u16); }
                    x &= x - 1;
                }
            }
            out
        }
    }
}

fn bitmap_op<'a>(a: Set<'a>, b: Set<'a>, op: u8, n: usize) -> Set<'a> {
    let (ab, bb) = (if let Set::Bits(x) = &a { Some(x.as_ref()) } else { None }, if let Set::Bits(x) = &b { Some(x.as_ref()) } else { None });
    if ab.is_none() && bb.is_none() {
        let (Set::List(x), Set::List(y)) = (a, b) else { unreachable!() };
        return Set::List(Cow::Owned(merge_lists(&x, &y, op)));
    }
    let (al, bl) = (if let Set::List(x) = &a { Some(x.as_ref()) } else { None }, if let Set::List(x) = &b { Some(x.as_ref()) } else { None });
    let mut out = vec![0u64; n.div_ceil(64)];
    match (ab, al, bb, bl) {
        (Some(x), _, Some(y), _) => for i in 0..out.len() { out[i] = match op { 0 => x[i] & y[i], 1 => x[i] | y[i], _ => x[i] & !y[i] }; },
        (Some(x), _, None, Some(y)) => {
            if op != 0 { out.copy_from_slice(x); }
            for &id in y { let w = id as usize / 64; let bit = 1u64 << (id % 64); if op == 0 { if x[w] & bit != 0 { out[w] |= bit; } } else if op == 1 { out[w] |= bit; } else { out[w] &= !bit; } }
        }
        (None, Some(x), Some(y), _) => {
            for &id in x { let w = id as usize / 64; let bit = 1u64 << (id % 64); if op == 0 { if y[w] & bit != 0 { out[w] |= bit; } } else if op == 1 { out[w] |= bit; } else if y[w] & bit == 0 { out[w] |= bit; } }
            if op == 1 { for (o, &v) in out.iter_mut().zip(y) { *o |= v; } }
        }
        _ => unreachable!(),
    }
    if op == 1 && n % 64 != 0 { *out.last_mut().unwrap() &= (1u64 << (n % 64)) - 1; }
    if out.iter().map(|x| x.count_ones() as usize).sum::<usize>() < n / 64 {
        Set::List(Cow::Owned(as_vec(Set::Bits(Cow::Owned(out)), n)))
    } else { Set::Bits(Cow::Owned(out)) }
}

fn contains(shard: &Shard, term: usize, local: u16) -> bool {
    let rep = shard.reps[term];
    if rep.kind == BITMAP { shard.bits[rep.offset as usize + local as usize / 64] & (1u64 << (local % 64)) != 0 }
    else { let start = rep.offset as usize; shard.lists[start..start + rep.df as usize].binary_search(&local).is_ok() }
}

fn offer_top(result: &mut QueryResult, id: u32) {
    let len = result.len;
    let at = match result.first[..len].binary_search(&id) { Ok(_) => return, Err(at) => at };
    if at >= 10 { return; }
    let new_len = (len + 1).min(10);
    for i in (at + 1..new_len).rev() { result.first[i] = result.first[i - 1]; }
    result.first[at] = id;
    result.len = new_len;
}

fn offer(result: &mut QueryResult, id: u32) {
    result.count += 1;
    result.sum += id as u64;
    // Shards and every set iterator are in ascending document order, so the local first-ten list is append-only.
    if result.len < 10 { result.first[result.len] = id; result.len += 1; }
}

fn summarize(set: Set<'_>, shard: &Shard, result: &mut QueryResult) {
    match set {
        Set::List(ids) => for &local in ids.iter() { offer(result, shard.ids[local as usize]); },
        Set::Bits(bits) => for (w, &word) in bits.iter().enumerate() {
            let mut x = word;
            while x != 0 { let local = w * 64 + x.trailing_zeros() as usize; if local < shard.ids.len() { offer(result, shard.ids[local]); } x &= x - 1; }
        },
    }
}

fn eval_query(q: &base::Query, shard: &Shard, terms: &[base::Term], ids: &[usize], ops: &[u8], docs: &[u8], query_timing: bool, scratch_a: &mut Vec<u16>, scratch_b: &mut Vec<u16>, bits_scratch: &mut Vec<u64>) -> (QueryResult, usize, u64) {
    let mut result = QueryResult::default();
    match q {
        base::Query::Bool(range, op_range) => {
            let query_terms = &ids[range.clone()];
            if query_terms.is_empty() { return (result, 0, 0); }
            let class = usize::from(shard.reps[query_terms[0]].kind == BITMAP);
            let start = if query_timing { thread_cpu_ns() } else { 0 };
            if query_terms.iter().all(|&t| shard.reps[t].kind == BITMAP) {
                bits_scratch.resize(shard.words, 0);
                let first = shard.reps[query_terms[0]];
                let begin = first.offset as usize;
                bits_scratch.copy_from_slice(&shard.bits[begin..begin + shard.words]);
                for (i, &t) in query_terms.iter().enumerate().skip(1) {
                    let rep = shard.reps[t];
                    let row = &shard.bits[rep.offset as usize..rep.offset as usize + shard.words];
                    let op = ops[op_range.start + i - 1];
                    for (left, &right) in bits_scratch.iter_mut().zip(row) {
                        *left = match op { 0 => *left & right, 1 => *left | right, _ => *left & !right };
                    }
                }
                for (w, &word) in bits_scratch.iter().enumerate() {
                    let mut x = word;
                    while x != 0 { let local = w * 64 + x.trailing_zeros() as usize; if local < shard.ids.len() { offer(&mut result, shard.ids[local]); } x &= x - 1; }
                }
                return (result, class, if query_timing { thread_cpu_ns() - start } else { 0 });
            }
            if query_terms.iter().all(|&t| shard.reps[t].kind == LIST) {
                let first = term_set(shard, query_terms[0]);
                let Set::List(first) = first else { unreachable!() };
                let mut current = 0u8;
                for (i, &t) in query_terms.iter().enumerate().skip(1) {
                    let next = term_set(shard, t);
                    let Set::List(next) = next else { unreachable!() };
                    let op = ops[op_range.start + i - 1];
                    match current {
                        0 => { merge_lists_into(&first, &next, op, scratch_a); current = 1; }
                        1 => { merge_lists_into(scratch_a, &next, op, scratch_b); current = 2; }
                        _ => { merge_lists_into(scratch_b, &next, op, scratch_a); current = 1; }
                    }
                }
                let final_list = match current { 0 => &*first, 1 => &*scratch_a, _ => &*scratch_b };
                for &local in final_list { offer(&mut result, shard.ids[local as usize]); }
                return (result, class, if query_timing { thread_cpu_ns() - start } else { 0 });
            }
            let mut set = term_set(shard, query_terms[0]);
            for (i, &t) in query_terms.iter().enumerate().skip(1) { set = bitmap_op(set, term_set(shard, t), ops[op_range.start + i - 1], shard.ids.len()); }
            summarize(set, shard, &mut result);
            (result, class, if query_timing { thread_cpu_ns() - start } else { 0 })
        }
        base::Query::Phrase(range) => {
            let query_terms = &ids[range.clone()];
            if query_terms.is_empty() { return (result, 2, 0); }
            let anchor = *query_terms.iter().min_by_key(|&&t| shard.reps[t].df).unwrap();
            let start = if query_timing { thread_cpu_ns() } else { 0 };
            let (text_starts, text_ends) = (&shard.text, &shard.ids);
            let candidate = |local: usize, result: &mut QueryResult| {
                let local = local as u16;
                if query_terms.iter().all(|&t| contains(shard, t, local)) {
                    let (s, e) = text_starts[local as usize];
                    if base::phrase_in_doc(&docs[s as usize..e as usize], query_terms, anchor, terms) {
                        offer(result, text_ends[local as usize]);
                    }
                }
            };
            let rep = shard.reps[anchor];
            if rep.kind == LIST {
                let start = rep.offset as usize;
                for &local in &shard.lists[start..start + rep.df as usize] { candidate(local as usize, &mut result); }
            } else {
                for local in 0..shard.ids.len() { if contains(shard, anchor, local as u16) { candidate(local, &mut result); } }
            }
            (result, 2, if query_timing { thread_cpu_ns() - start } else { 0 })
        }
    }
}

fn push_num(out: &mut Vec<u8>, mut n: u64) {
    let mut buf = [0u8; 20];
    let mut p = buf.len();
    loop { p -= 1; buf[p] = b'0' + (n % 10) as u8; n /= 10; if n == 0 { break; } }
    out.extend_from_slice(&buf[p..]);
}

fn write_result(out: &mut Vec<u8>, r: QueryResult) {
    push_num(out, r.count); out.push(b' '); push_num(out, r.sum);
    for &id in &r.first[..r.len] { out.push(b' '); push_num(out, id as u64); }
    out.push(b'\n');
}

fn main() -> io::Result<()> {
    let timing = std::env::var_os("SD_TIMING").is_some();
    let query_timing = std::env::var_os("SD_QTIMING").is_some();
    let total = Instant::now();
    let mut phase = [0u128; 7];
    let snap = |name: &str| { if timing { let (a, h, f) = proc_mem(); eprintln!("SD_MEM phase={name} anon={a} hwm={h} file={f}"); } };
    let args: Vec<String> = std::env::args().collect();
    let root = args.get(1).map_or("/data", String::as_str);
    let output = args.get(2).map_or("/out", String::as_str);
    let (_file, ptr, len) = map_file(Path::new(&format!("{root}/docs.txt")))?;
    // SAFETY: the slice covers the live read-only mmap and remains valid until the mapping owner is dropped.
    let docs = unsafe { std::slice::from_raw_parts(ptr, len) };
    let advise_ptr = ptr as usize;
    let prefetch = std::thread::spawn(move || advise(advise_ptr, len));
    let qread = Instant::now();
    let qdata = std::fs::read(format!("{root}/queries.txt"))?;
    let query_read = qread.elapsed().as_micros();
    let parse = Instant::now();
    let (terms, queries, query_ids, query_ops) = base::parse_queries(&qdata);
    phase[0] = parse.elapsed().as_micros();
    prefetch.join().expect("mmap advice thread panicked");
    let build = Instant::now();
    let table = base::ScanTable::new(&terms);
    phase[1] = build.elapsed().as_micros();
    let n = doc_count(docs);
    let mut wanted = 12usize.max(n.div_ceil(u16::MAX as usize));
    wanted = wanted.min(n.max(1));
    let setup = Instant::now();
    let mut shard_ranges = ranges(docs, wanted);
    phase[3] = setup.elapsed().as_micros();
    let scan_faults_before = if timing { faults() } else { (0, 0) };
    let count_start = Instant::now();
    let terms_ref = &terms;
    let table_ref = &table;
    let (shard_ranges, plans): (Vec<(usize, usize)>, Vec<Plan>) = loop {
        let plans: Vec<_> = std::thread::scope(|scope| {
            let handles: Vec<_> = shard_ranges.iter().copied().map(|range| scope.spawn(move || count_shard(docs, range, terms_ref, table_ref))).collect();
            handles.into_iter().map(|h| h.join().expect("shard count pass panicked")).collect()
        });
        if plans.iter().all(|plan| plan.ids.len() <= u16::MAX as usize) { break (shard_ranges, plans); }
        wanted += 1;
        drop(plans);
        // If a byte-balanced shard exceeds u16, split further and recount so local IDs remain exact.
        let split_start = Instant::now();
        shard_ranges = ranges(docs, wanted);
        phase[3] += split_start.elapsed().as_micros();
    };
    let workers = shard_ranges.len();
    snap("count_pass_peak");
    let mut shards: Vec<Shard> = plans.into_iter().map(|plan| allocate_shard(plan, &terms)).collect();
    std::thread::scope(|scope| {
        let handles: Vec<_> = shards.iter_mut().map(|shard| scope.spawn(move || { let elapsed = fill_shard(docs, shard, terms_ref, table_ref); shard.fill_us = elapsed; })).collect();
        for h in handles { h.join().expect("shard fill pass panicked"); }
    });
    phase[4] = count_start.elapsed().as_micros();
    let scan_faults_after = if timing { faults() } else { (0, 0) };
    let scan_faults = (scan_faults_after.0 - scan_faults_before.0, scan_faults_after.1 - scan_faults_before.1);
    snap("index_peak");
    let worker_times: Vec<u128> = shards.iter().map(|s| s.count_us + s.fill_us).collect();
    let rep_bytes: usize = shards.iter().map(|s| s.reps.len() * std::mem::size_of::<Rep>()).sum();
    let list_bytes: usize = shards.iter().map(|s| s.lists.len() * 2).sum();
    let bitmap_bytes: usize = shards.iter().map(|s| s.bits.len() * 8).sum();
    let doc_bytes: usize = shards.iter().map(|s| s.ids.len() * 4 + s.text.len() * 8).sum();
    if timing { eprintln!("SD_INDEX reps={} lists={} bitmaps={} doc_meta={}", rep_bytes, list_bytes, bitmap_bytes, doc_bytes); }
    let mut shard_order: Vec<usize> = (0..shards.len()).collect();
    shard_order.sort_unstable_by_key(|&i| shards[i].ids.iter().copied().min().unwrap_or(u32::MAX));
    let disjoint_id_ranges = shard_order.windows(2).all(|w| {
        shards[w[0]].ids.iter().copied().max().unwrap_or(0) < shards[w[1]].ids.iter().copied().min().unwrap_or(u32::MAX)
    });
    let query_start = Instant::now();
    let mut out = Vec::with_capacity(queries.len() * 24);
    let mut query_cpu = [0u64; 3];
    if workers == 0 {
        for _ in &queries { write_result(&mut out, QueryResult::default()); }
    } else {
        let barrier = Barrier::new(workers + 1);
        let (tx, rx) = mpsc::channel();
        std::thread::scope(|scope| {
            let terms_ref = &terms;
            let queries_ref = &queries;
            let query_ids_ref = &query_ids;
            let query_ops_ref = &query_ops;
            for (sid, shard) in shards.iter().enumerate() {
                let tx = tx.clone(); let barrier = &barrier;
                let shard_queries = queries_ref;
                let shard_terms = terms_ref;
                let shard_query_ids = query_ids_ref;
                let shard_query_ops = query_ops_ref;
                scope.spawn(move || {
                    let mut scratch_a = Vec::new();
                    let mut scratch_b = Vec::new();
                    let mut bits_scratch = Vec::new();
                    for block in (0..shard_queries.len()).step_by(QUERY_BLOCK) {
                        let end = (block + QUERY_BLOCK).min(shard_queries.len());
                        let mut results = Vec::with_capacity(end - block);
                        let mut cpu = [0u64; 3];
                        for q in &shard_queries[block..end] {
                            let (result, class, elapsed) = eval_query(q, shard, shard_terms, shard_query_ids, shard_query_ops, docs, query_timing, &mut scratch_a, &mut scratch_b, &mut bits_scratch);
                            results.push(result); cpu[class] += elapsed;
                        }
                        tx.send((sid, block, results, cpu)).expect("query receiver dropped");
                        barrier.wait();
                    }
                });
            }
            drop(tx);
            for block in (0..queries.len()).step_by(QUERY_BLOCK) {
                let end = (block + QUERY_BLOCK).min(queries.len());
                let mut parts: Vec<Option<Vec<QueryResult>>> = (0..workers).map(|_| None).collect();
                for _ in 0..workers {
                    let (sid, found_block, local, cpu) = rx.recv().expect("query shard exited");
                    assert_eq!(found_block, block);
                    query_cpu[0] += cpu[0]; query_cpu[1] += cpu[1]; query_cpu[2] += cpu[2];
                    parts[sid] = Some(local);
                }
                let mut merged = vec![QueryResult::default(); end - block];
                for q in 0..merged.len() {
                    for part in parts.iter().flatten() { merged[q].count += part[q].count; merged[q].sum += part[q].sum; }
                    if disjoint_id_ranges {
                        for &sid in &shard_order {
                            let part = &parts[sid].as_ref().unwrap()[q];
                            for &id in &part.first[..part.len] {
                                if merged[q].len == 10 { break; }
                                let target = &mut merged[q];
                                target.first[target.len] = id; target.len += 1;
                            }
                            if merged[q].len == 10 { break; }
                        }
                    } else {
                        for part in parts.iter().flatten() { for &id in &part[q].first[..part[q].len] { offer_top(&mut merged[q], id); } }
                    }
                }
                for result in merged { write_result(&mut out, result); }
                snap("query_block_peak");
                barrier.wait();
            }
        });
    }
    phase[5] = query_start.elapsed().as_micros();
    if query_timing { eprintln!("SD_QCPU bool_list={} bool_bitmap={} phrase={}", query_cpu[0], query_cpu[1], query_cpu[2]); }
    let write = Instant::now();
    std::fs::create_dir_all(output)?;
    std::fs::write(Path::new(output).join("results.txt"), out)?;
    phase[6] = write.elapsed().as_micros();
    if timing {
        let rss = proc_mem();
        eprintln!("SD_TIMING read={} query_parse={} parse={} table={} scan_a={} setup={} scan_b={} queries={} output={} total={} rss_anon_kb={} vmhwm_kb={} rss_file_kb={} scan_minor={} scan_major={} workers={:?}", query_read, phase[0], phase[0], phase[1], 0, phase[3], phase[4], phase[5], phase[6], total.elapsed().as_micros(), rss.0, rss.1, rss.2, scan_faults.0, scan_faults.1, worker_times);
    }
    std::process::exit(0)
}
