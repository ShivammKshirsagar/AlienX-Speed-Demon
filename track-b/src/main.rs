use std::borrow::Cow;
use std::{fs::File, io::{self, Write}, os::fd::AsRawFd, path::Path, sync::atomic::{AtomicU64, AtomicUsize, Ordering}, time::Instant};

#[repr(C)]
pub struct Term {
    bytes: Vec<u8>,
    hash: u64,
    df: usize,
    bitmap: bool,
    offset: usize,
}
pub enum Query {
    Bool(Vec<usize>, Vec<u8>),
    Phrase(Vec<usize>),
}
enum Set<'a> {
    List(Vec<u32>),
    Bits(Cow<'a, [u64]>),
}
struct Postings { data: Vec<u32>, starts: Vec<usize> }

fn folded(x: u8) -> u8 { if x.is_ascii_uppercase() { x + 32 } else { x } }
fn is_word(x: u8) -> bool { x.is_ascii_lowercase() || x.is_ascii_digit() }
pub fn scan_document_tokens<F: FnMut(&[u8])>(line: &[u8], visit: F) {
    if let Some(tab) = line.iter().position(|&b| b == b'\t') { scan_tokens(&line[tab + 1..], visit); }
}
#[cfg(target_feature = "avx2")]
fn word_mask32(bytes: &[u8]) -> u32 {
    use std::arch::x86_64::*;
    // SAFETY: callers pass a slice with at least 32 readable bytes and the build enables AVX2.
    unsafe {
        let v = _mm256_loadu_si256(bytes.as_ptr().cast());
        let lower = _mm256_or_si256(v, _mm256_set1_epi8(0x20));
        let letters = _mm256_and_si256(
            _mm256_cmpgt_epi8(lower, _mm256_set1_epi8(0x60)),
            _mm256_cmpgt_epi8(_mm256_set1_epi8(0x7b), lower),
        );
        let digits = _mm256_and_si256(
            _mm256_cmpgt_epi8(v, _mm256_set1_epi8(0x2f)),
            _mm256_cmpgt_epi8(_mm256_set1_epi8(0x3a), v),
        );
        _mm256_movemask_epi8(_mm256_or_si256(letters, digits)) as u32
    }
}
#[cfg(target_feature = "avx2")]
pub fn scan_tokens_avx2_chunks<F: FnMut(&[u8])>(data: &[u8], chunk_bytes: usize, mut visit: F) {
    let chunk_bytes = chunk_bytes.max(1);
    let mut token_start = None;
    let mut carry = 0u32;
    let mut chunk_start = 0usize;
    while chunk_start < data.len() {
        let chunk_end = data.len().min(chunk_start.saturating_add(chunk_bytes));
        let mut base = chunk_start;
        while base + 32 <= chunk_end {
            let mask = word_mask32(&data[base..base + 32]);
            // Starts and ends are bit transitions; carry represents the preceding byte across block boundaries.
            let mut starts = mask & !((mask << 1) | carry);
            let mut ends = !mask & ((mask << 1) | carry);
            while starts != 0 || ends != 0 {
                let s = starts.trailing_zeros();
                let e = ends.trailing_zeros();
                if starts != 0 && (ends == 0 || s < e) {
                    token_start = Some(base + s as usize);
                    starts &= starts - 1;
                } else {
                    if let Some(start) = token_start.take() {
                        visit(&data[start..base + e as usize]);
                    }
                    ends &= ends - 1;
                }
            }
            carry = mask >> 31;
            base += 32;
        }
        while base < chunk_end {
            let word = is_word(folded(data[base]));
            if word {
                if token_start.is_none() {
                    token_start = Some(base);
                }
            } else if let Some(start) = token_start.take() {
                visit(&data[start..base]);
            }
            carry = u32::from(word);
            base += 1;
        }
        chunk_start = chunk_end;
    }
    if let Some(start) = token_start {
        visit(&data[start..]);
    }
}
pub fn scan_tokens<F: FnMut(&[u8])>(data: &[u8], visit: F) {
    scan_tokens_avx2_chunks(data, data.len().max(1), visit);
}
fn hash_bytes(raw: &[u8]) -> u64 {
    raw.iter().fold(0xcbf29ce484222325, |h, &x| (h ^ folded(x) as u64).wrapping_mul(0x100000001b3))
}
#[derive(Clone, Copy, Default)]
struct ScanSlot {
    key0: u64,
    key1: u64,
    id_plus_one: usize,
    len: usize,
}
struct ScanKey {
    key0: u64,
    key1: u64,
    hash: u64,
}
fn folded_word(raw: &[u8], start: usize) -> u64 {
    let tail = &raw[start..];
    if tail.len() >= 8 {
        // SAFETY: at least eight token bytes remain at `start`; unaligned reads are allowed.
        unsafe { (tail.as_ptr().cast::<u64>().read_unaligned()).to_le() | 0x2020202020202020 }
    } else {
        let mut bytes = [0u8; 8];
        for (dst, &src) in bytes.iter_mut().zip(tail) { *dst = folded(src); }
        u64::from_le_bytes(bytes)
    }
}
fn scan_key(raw: &[u8]) -> ScanKey {
    let len = raw.len();
    if len <= 16 {
        let key0 = folded_word(raw, 0);
        let key1 = if len >= 8 { folded_word(raw, len - 8) } else { 0 };
        let hash = (key0 ^ key1.rotate_left(17) ^ len as u64).wrapping_mul(0x9e3779b185ebca87);
        ScanKey { key0, key1, hash: hash ^ (hash >> 32) }
    } else { let hash = hash_bytes(raw); ScanKey { key0: hash, key1: 0, hash } }
}
pub struct ScanTable {
    slots: Vec<ScanSlot>,
}
impl ScanTable {
    pub fn new(terms: &[Term]) -> Self {
        let mut capacity = 1usize;
        while capacity < terms.len().saturating_mul(3).div_ceil(2) { capacity *= 2; }
        let mut slots = vec![ScanSlot::default(); capacity];
        for (id, term) in terms.iter().enumerate() {
            let key = scan_key(&term.bytes);
            let mut p = key.hash as usize & (capacity - 1);
            while slots[p].id_plus_one != 0 { p = (p + 1) & (capacity - 1); }
            slots[p] = ScanSlot { key0: key.key0, key1: key.key1, id_plus_one: id + 1, len: term.bytes.len() };
        }
        Self { slots }
    }
}
#[derive(Clone)]
struct HotTable {
    keys: Vec<(u64, u64)>,
    ids: Vec<u16>,
}
impl HotTable {
    fn new(terms: &[Term], ids: &[usize]) -> Self {
        let mut h = Self { keys: vec![(0, 0); 2048], ids: vec![u16::MAX; 2048] };
        for (i, &t) in ids.iter().enumerate() {
            let k = scan_key(&terms[t].bytes);
            let mut p = k.hash as usize & 2047;
            while h.ids[p] != u16::MAX { p = (p + 1) & 2047; }
            h.keys[p] = (k.key0, k.key1);
            h.ids[p] = i as u16;
        }
        h
    }
    fn lookup(&self, raw: &[u8]) -> Option<usize> {
        if raw.len() > 16 { return None; }
        let k = scan_key(raw);
        let mut p = k.hash as usize & 2047;
        loop {
            let id = self.ids[p];
            if id == u16::MAX { return None; }
            if self.keys[p] == (k.key0, k.key1) { return Some(id as usize); }
            p = (p + 1) & 2047;
        }
    }
}
pub fn lookup(terms: &[Term], table: &ScanTable, raw: &[u8]) -> Option<usize> {
    if raw.is_empty() { return None; }
    let key = scan_key(raw);
    let mut p = key.hash as usize & (table.slots.len() - 1);
    loop {
        let slot = table.slots[p];
        if slot.id_plus_one == 0 {
            return None;
        }
        if slot.len == raw.len() && slot.key0 == key.key0 && slot.key1 == key.key1 {
            let id = slot.id_plus_one - 1;
            if raw.len() <= 16 || terms[id].bytes.iter().zip(raw).all(|(&a, &b)| a == folded(b)) {
                return Some(id);
            }
        }
        p = (p + 1) & (table.slots.len() - 1);
    }
}
fn intern(raw: &[u8], terms: &mut Vec<Term>, slots: &mut Vec<u32>) -> usize {
    let h = raw.iter().fold(0xcbf29ce484222325, |h, &x| (h ^ x as u64).wrapping_mul(0x100000001b3));
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
pub fn parse_queries(data: &[u8]) -> (Vec<Term>, Vec<Query>) {
    let mut cap = 1usize;
    while cap < data.len() / 4 { cap *= 2; }
    let mut slots = vec![0u32; cap];
    let mut terms = Vec::new();
    let mut queries = Vec::new();
    for line in data.split(|&b| b == b'\n').filter(|x| !x.is_empty()) {
        let mut p = 0;
        while p < line.len() && line[p] != b' ' { p += 1; }
        let head = &line[..p];
        if head == b"BOOL" {
            let mut ids = Vec::new();
            let mut ops = Vec::new();
            let mut q = p + 1;
            while q < line.len() {
                let s = q;
                while q < line.len() && line[q] != b' ' { q += 1; }
                ids.push(intern(&line[s..q], &mut terms, &mut slots));
                if q < line.len() {
                    q += 1;
                    let op = &line[q..];
                    if op.starts_with(b"OR") {
                        ops.push(1); q += 2;
                    } else if op.starts_with(b"NOT") {
                        ops.push(2); q += 3;
                    } else {
                        ops.push(0); q += 3;
                    }
                    if q < line.len() { q += 1; }
                }
            }
            queries.push(Query::Bool(ids, ops));
        } else {
            let s = p + 2;
            let e = line.len() - 1;
            let mut ids = Vec::new();
            for t in line[s..e].split(|&b| b == b' ') { ids.push(intern(t, &mut terms, &mut slots)); }
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
    if p as isize == -1 { return Err(io::Error::last_os_error()); }
    Ok((f, p.cast(), n))
}
fn merge(a: &[u32], b: &[u32], op: u8) -> Vec<u32> {
    let mut out = Vec::with_capacity(if op == 1 { a.len() + b.len() } else { a.len().min(b.len()) });
    let (mut i, mut j) = (0, 0);
    while i < a.len() && j < b.len() {
        if a[i] == b[j] {
            if op != 2 { out.push(a[i]); } i += 1; j += 1;
        } else if a[i] < b[j] {
            if op != 0 { out.push(a[i]); } i += 1;
        } else {
            if op == 1 { out.push(b[j]); } j += 1;
        }
    }
    if op != 0 { out.extend_from_slice(&a[i..]); }
    if op == 1 { out.extend_from_slice(&b[j..]); }
    out
}
fn as_list(s: Set<'_>, n: usize) -> Vec<u32> {
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
fn bitmap_op(a: Set<'_>, b: Set<'_>, op: u8, n: usize) -> Set<'static> {
    let words = (n + 63) / 64;
    let aa = match &a {
        Set::Bits(x) => Some(x.as_ref()),
        _ => None,
    };
    let bb = match &b {
        Set::Bits(x) => Some(x.as_ref()),
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
        Set::List(as_list(Set::Bits(Cow::Owned(out)), n))
    } else {
        Set::Bits(Cow::Owned(out))
    }
}
impl Postings {
    fn list(&self, t: usize, terms: &[Term]) -> &[u32] { &self.data[self.starts[t]..self.starts[t] + terms[t].df] }
}
fn term_set<'a>(t: usize, terms: &[Term], posts: &Postings, bits: &'a [u64], n: usize) -> Set<'a> {
    let x = &terms[t];
    if x.bitmap {
        let w = (n + 63) / 64;
        Set::Bits(Cow::Borrowed(&bits[x.offset..x.offset + w]))
    } else { Set::List(posts.list(t, terms).to_vec()) }
}
fn contains_doc(t: usize, doc: u32, terms: &[Term], posts: &Postings, bits: &[u64]) -> bool {
    let term = &terms[t];
    if term.bitmap {
        bits[term.offset + doc as usize / 64] & (1u64 << (doc % 64)) != 0
    } else { posts.list(t, terms).binary_search(&doc).is_ok() }
}
fn same_token(raw: &[u8], term: &[u8]) -> bool { raw.len() == term.len() && raw.iter().zip(term).all(|(&a, &b)| folded(a) == folded(b)) }
fn phrase_matches_at(data: &[u8], at: usize, anchor_pos: usize, ids: &[usize], terms: &[Term]) -> bool {
    let anchor = &terms[ids[anchor_pos]].bytes;
    let anchor_end = at + anchor.len();
    if (at != 0 && is_word(folded(data[at - 1])))
        || anchor_end > data.len()
        || (anchor_end < data.len() && is_word(folded(data[anchor_end])))
        || !same_token(&data[at..anchor_end], anchor)
    {
        return false;
    }
    let mut cursor = at;
    for q in (0..anchor_pos).rev() {
        while cursor > 0 && !is_word(folded(data[cursor - 1])) {
            cursor -= 1;
        }
        let end = cursor;
        while cursor > 0 && is_word(folded(data[cursor - 1])) {
            cursor -= 1;
        }
        if cursor == end || !same_token(&data[cursor..end], &terms[ids[q]].bytes) {
            return false;
        }
    }
    cursor = anchor_end;
    for &id in &ids[anchor_pos + 1..] {
        while cursor < data.len() && !is_word(folded(data[cursor])) {
            cursor += 1;
        }
        let start = cursor;
        while cursor < data.len() && is_word(folded(data[cursor])) {
            cursor += 1;
        }
        if cursor == start || !same_token(&data[start..cursor], &terms[id].bytes) {
            return false;
        }
    }
    true
}
#[cfg(target_feature = "avx2")]
fn anchor_mask32(bytes: &[u8], first: u8) -> u32 {
    use std::arch::x86_64::*;
    // SAFETY: caller supplies 32 readable bytes and the build enables AVX2.
    unsafe {
        let v = _mm256_loadu_si256(bytes.as_ptr().cast());
        let compared = if first.is_ascii_alphabetic() {
            // ASCII letters are folded with OR 0x20; digits use exact comparison below.
            _mm256_or_si256(v, _mm256_set1_epi8(0x20))
        } else {
            v
        };
        _mm256_movemask_epi8(_mm256_cmpeq_epi8(compared, _mm256_set1_epi8(first as i8))) as u32
    }
}
pub fn phrase_in_doc(data: &[u8], ids: &[usize], anchor: usize, terms: &[Term]) -> bool {
    let first = folded(terms[anchor].bytes[0]);
    let mut anchor_positions = [0usize; 8];
    let mut count = 0;
    for (i, &id) in ids.iter().enumerate() {
        if id == anchor {
            anchor_positions[count] = i;
            count += 1;
        }
    }
    let mut base = 0;
    while base < data.len() {
        #[cfg(target_feature = "avx2")]
        if base + 32 <= data.len() {
            let mut candidates = anchor_mask32(&data[base..base + 32], first);
            while candidates != 0 {
                let at = base + candidates.trailing_zeros() as usize;
                for &anchor_pos in &anchor_positions[..count] {
                    if phrase_matches_at(data, at, anchor_pos, ids, terms) {
                        return true;
                    }
                }
                candidates &= candidates - 1;
            }
            base += 32;
            continue;
        }
        if folded(data[base]) == first {
            for &anchor_pos in &anchor_positions[..count] {
                if phrase_matches_at(data, base, anchor_pos, ids, terms) {
                    return true;
                }
            }
        }
        base += 1;
    }
    false
}
fn output_set(s: Set<'_>, n: usize, out: &mut Vec<u8>) {
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
    while tab < line.len() && line[tab] != b'\t' { id = id * 10 + (line[tab] - b'0') as usize; tab += 1; }
    (id, tab)
}
fn doc_count(docs: &[u8]) -> usize {
    if docs.is_empty() { return 0; }
    let end = docs.len() - usize::from(docs.last() == Some(&b'\n'));
    let start = docs[..end].iter().rposition(|&b| b == b'\n').map_or(0, |p| p + 1);
    document_id(&docs[start..end]).0 + 1
}
fn sample_hot(docs: &[u8], terms: &[Term], table: &ScanTable) -> Vec<usize> {
    let mut counts = vec![0u32; terms.len()];
    let mut last = vec![u32::MAX; terms.len()];
    let size = 85 * 1024;
    let ranges: Vec<_> = if docs.len() <= size * 12 {
        vec![(0, docs.len())]
    } else {
        (0..12)
            .filter_map(|i| {
                let mut s = docs.len() * i / 12;
                while s < docs.len() && s > 0 && docs[s - 1] != b'\n' {
                    s += 1;
                }
                if s == docs.len() {
                    return None;
                }
                let mut e = (s + size).min(docs.len());
                while e < docs.len() && docs[e - 1] != b'\n' {
                    e += 1;
                }
                Some((s, e))
            })
            .collect()
    };
    for (s, e) in ranges {
        let mut p = s;
        while p < e {
            let end = docs[p..e].iter().position(|&b| b == b'\n').map_or(e, |x| p + x);
            let line = &docs[p..end];
            let id = document_id(line).0 as u32;
            scan_document_tokens(line, |tok| {
                if let Some(t) = lookup(terms, table, tok) {
                    if last[t] != id {
                        last[t] = id;
                        counts[t] += 1;
                    }
                }
            });
            p = end + 1;
        }
    }
    let mut ids: Vec<_> = (0..terms.len()).filter(|&t| counts[t] >= 2 && terms[t].bytes.len() <= 16).collect();
    ids.sort_unstable_by_key(|&t| (std::cmp::Reverse(counts[t]), t));
    ids.truncate(1024);
    ids
}
struct WorkerScan {
    counts: Vec<u32>,
    pairs: Vec<(u32, u32)>,
    rows: usize,
}
fn scan_onepass(
    docs: &[u8],
    start: usize,
    end: usize,
    terms: &[Term],
    table: &ScanTable,
    hot: &HotTable,
    term_hot: &[u16],
    hot_len: usize,
    words: usize,
    bits: &[AtomicU64],
    text_ptr: usize,
) -> WorkerScan {
    let (mut last, mut counts) = (vec![u32::MAX; terms.len()], vec![0u32; terms.len()]);
    let mut hot_last = vec![u32::MAX; hot_len];
    // Reserve about one pair per 24 source bytes, matching the observed cold-hit density.
    let mut pairs = Vec::with_capacity((end - start) / 24);
    let (mut pos, mut rows) = (start, 0);
    while pos < end {
        let end_line = docs[pos..end].iter().position(|&b| b == b'\n').map_or(end, |x| pos + x);
        let line = &docs[pos..end_line];
        let (raw_id, tab) = document_id(line);
        let id = raw_id as u32;
        // SAFETY: dense document IDs are unique and each worker owns a disjoint newline-aligned range.
        unsafe {
            (text_ptr as *mut u32).add(id as usize).write((pos + tab + 1) as u32);
        }
        scan_document_tokens(line, |tok| {
            if let Some(h) = hot.lookup(tok) {
                if hot_last[h] != id {
                    hot_last[h] = id;
                    bits[h * words + id as usize / 64].fetch_or(1 << (id % 64), Ordering::Relaxed);
                }
            } else if let Some(t) = lookup(terms, table, tok) {
                let h = term_hot[t];
                if h != u16::MAX {
                    let h = h as usize;
                    if hot_last[h] != id {
                        hot_last[h] = id;
                        bits[h * words + id as usize / 64].fetch_or(1 << (id % 64), Ordering::Relaxed);
                    }
                } else if last[t] != id {
                    last[t] = id;
                    counts[t] += 1;
                    pairs.push((t as u32, id));
                }
            }
        });
        rows += 1;
        pos = end_line + 1;
    }
    // Group each worker's sequential pair buffer by term and document so scatter writes are contiguous and sorted.
    pairs.sort_unstable_by_key(|&(t, id)| ((t as u64) << 32) | id as u64);
    WorkerScan { counts, pairs, rows }
}
fn eval_query(
    q: &Query,
    terms: &[Term],
    _table: &ScanTable,
    posts: &Postings,
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
            let mut ordered = [0usize; 8];
            ordered[..ids.len()].copy_from_slice(ids);
            ordered[..ids.len()].sort_unstable_by_key(|&t| terms[t].df);
            let mut order_len = 0;
            for i in 0..ids.len() {
                if i == 0 || ordered[i] != ordered[i - 1] {
                    ordered[order_len] = ordered[i];
                    order_len += 1;
                }
            }
            let anchor = ordered[0];
            let candidates = as_list(term_set(anchor, terms, posts, bits, n), n);
            let mut found = Vec::new();
            for d in candidates {
                if ordered[1..order_len].iter().any(|&t| !contains_doc(t, d, terms, posts, bits)) {
                    continue;
                }
                let start = text_starts[d as usize] as usize;
                let end = docs[start..].iter().position(|&b| b == b'\n').map_or(docs.len(), |offset| start + offset);
                if phrase_in_doc(&docs[start..end], ids, anchor, terms) {
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
#[rustfmt::skip]
fn main() -> io::Result<()> {
    let timing = std::env::var_os("SD_TIMING").is_some(); let total = Instant::now(); let mut lap = total; let mut phases = [0u128; 7];
    macro_rules! tick { ($i:expr) => { if timing { phases[$i] = lap.elapsed().as_micros(); lap = Instant::now(); } }; }
    let args: Vec<String> = std::env::args().collect();
    let root = args.get(1).map_or("/data", String::as_str); let output = args.get(2).map_or("/out", String::as_str);
    let qdata = std::fs::read(format!("{root}/queries.txt"))?; let (mut terms, queries) = parse_queries(&qdata);
    let (_file, ptr, len) = map_file(Path::new(&format!("{root}/docs.txt")))?;
    // SAFETY: the read-only slice covers the live mmap and does not outlive it.
    let docs = unsafe { std::slice::from_raw_parts(ptr, len) };
    tick!(0); let table = ScanTable::new(&terms); let hot_ids = sample_hot(docs, &terms, &table); tick!(1);
    let n = doc_count(docs);
    let mut term_hot = vec![u16::MAX; terms.len()];
    for (h, &t) in hot_ids.iter().enumerate() { term_hot[t] = h as u16; terms[t].bitmap = true; terms[t].offset = h * n.div_ceil(64); }
    let words = n.div_ceil(64); let hot_len = hot_ids.len();
    let bitmaps: Vec<_> = (0..hot_len * words).map(|_| AtomicU64::new(0)).collect();
    let mut text_starts = vec![0u32; n]; let text_ptr = text_starts.as_mut_ptr() as usize;
    let workers = std::thread::available_parallelism().map_or(1, usize::from).min(12);
    let mut boundaries = vec![0usize];
    for i in 1..workers {
        let mut s = len * i / workers;
        while s < len && s > 0 && docs[s - 1] != b'\n' { s += 1; }
        if s < len && s > *boundaries.last().unwrap() { boundaries.push(s); }
    }
    boundaries.push(len); let chunks: Vec<_> = boundaries.windows(2).map(|w| (w[0], w[1])).collect();
    let hot_tables = vec![HotTable::new(&terms, &hot_ids); chunks.len()];
    tick!(3); if timing { phases[2] = 0; }
    let scans: Vec<WorkerScan> = std::thread::scope(|scope| {
        let handles: Vec<_> = chunks.iter().enumerate().map(|(w, &(s, e))| {
            let (ts, st, ht, th, bm) = (&terms, &table, &hot_tables[w], &term_hot, &bitmaps);
            scope.spawn(move || scan_onepass(docs, s, e, ts, st, ht, th, hot_len, words, bm, text_ptr))
        }).collect();
        handles.into_iter().map(|h| h.join().expect("one-pass worker panicked")).collect()
    });
    debug_assert_eq!(scans.iter().map(|x| x.rows).sum::<usize>(), n);
    for (t, term) in terms.iter_mut().enumerate() {
        term.df = if term.bitmap {
            bitmaps[term.offset..term.offset + words].iter().map(|x| x.load(Ordering::Relaxed).count_ones() as usize).sum()
        } else { scans.iter().map(|x| x.counts[t] as usize).sum() };
    }
    let mut starts = vec![0u32; terms.len()]; let mut total_posts = 0u32;
    for (t, term) in terms.iter().enumerate() { if !term.bitmap { starts[t] = total_posts; total_posts += term.df as u32; } }
    let terms_len = terms.len(); let mut off = vec![0u32; chunks.len() * terms_len];
    for (t, term) in terms.iter().enumerate().filter(|(_, x)| !x.bitmap) {
        let mut next = starts[t];
        for (w, scan) in scans.iter().enumerate() { off[w * terms_len + t] = next; next += scan.counts[t]; }
        debug_assert_eq!(next, starts[t] + term.df as u32);
    }
    let mut data = vec![0u32; total_posts as usize]; let post_ptr = data.as_mut_ptr() as usize;
    std::thread::scope(|scope| {
        let handles: Vec<_> = scans.iter().enumerate().map(|(w, scan)| {
            let (pairs, offsets) = (&scan.pairs, &off);
            scope.spawn(move || {
                let mut cursor = vec![0u32; terms_len];
                for &(t, id) in pairs {
                    let t = t as usize; let slot = offsets[w * terms_len + t] + cursor[t];
                    // SAFETY: worker prefixes assign disjoint per-term slices; sorted pairs are unique.
                    unsafe { (post_ptr as *mut u32).add(slot as usize).write(id); }
                    cursor[t] += 1;
                }
            })
        }).collect();
        for h in handles { h.join().expect("scatter worker panicked"); }
    });
    drop(scans); tick!(4);
    let postings = Postings { data, starts: starts.into_iter().map(|x| x as usize).collect() };
    // SAFETY: AtomicU64 and u64 have identical layout; all atomic writers have joined.
    let bits = unsafe { std::slice::from_raw_parts(bitmaps.as_ptr().cast::<u64>(), bitmaps.len()) };
    let next = AtomicUsize::new(0); const BLOCK: usize = 64; let qlen = queries.len();
    let out = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..workers.min(qlen.max(1))).map(|_| {
            let (ts, st, ps, bm, starts) = (&terms, &table, &postings, bits, &text_starts); let next = &next; let qs = &queries;
            scope.spawn(move || {
                let mut blocks = Vec::new(); loop {
                    let s = next.fetch_add(BLOCK, Ordering::Relaxed); if s >= qlen { break; }
                    let mut part = Vec::new();
                    for q in &qs[s..(s + BLOCK).min(qlen)] { part.extend_from_slice(&eval_query(q, ts, st, ps, bm, docs, starts, n)); }
                    blocks.push((s / BLOCK, part));
                } blocks
            })
        }).collect();
        let mut blocks: Vec<_> = handles.into_iter().flat_map(|h| h.join().expect("query worker panicked")).collect();
        blocks.sort_unstable_by_key(|x| x.0); blocks.into_iter().flat_map(|x| x.1).collect::<Vec<u8>>()
    });
    tick!(5); std::fs::create_dir_all(output)?; std::fs::write(Path::new(output).join("results.txt"), out)?;
    if timing { phases[6] = lap.elapsed().as_micros(); let rss = std::fs::read_to_string("/proc/self/status").ok().and_then(|s| s.lines().find_map(|l| l.strip_prefix("RssAnon:")?.trim().split_whitespace().next()?.parse::<u64>().ok())).unwrap_or(0); eprintln!("SD_TIMING parse={} table={} scan_a={} setup={} scan_b={} queries={} output={} total={} rss_anon_kb={}", phases[0], phases[1], phases[2], phases[3], phases[4], phases[5], phases[6], total.elapsed().as_micros(), rss); }
    std::process::exit(0)
}
