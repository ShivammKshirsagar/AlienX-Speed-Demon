use std::borrow::Cow;
use std::{
    fs::File,
    io,
    os::fd::AsRawFd,
    path::Path,
    sync::atomic::{AtomicU64, AtomicUsize, Ordering},
    time::Instant,
};

pub struct Term {
    pub bytes: Vec<u8>,
    hash: u64,
    df: usize,
    bitmap: bool,
    offset: usize,
}
pub enum Query {
    Bool(std::ops::Range<usize>, std::ops::Range<usize>),
    Phrase(std::ops::Range<usize>),
}
enum Set<'a> {
    List(Cow<'a, [u32]>),
    Bits(Cow<'a, [u64]>),
}
struct CompressedRun {
    data: Vec<u8>,
    block_offsets: Vec<u32>,
    term_block_starts: Vec<u32>,
}
struct Postings {
    runs: Vec<CompressedRun>,
    bases: Vec<u32>,
    ends: Vec<u32>,
}

fn folded(x: u8) -> u8 {
    if x.is_ascii_uppercase() {
        x + 32
    } else {
        x
    }
}
fn is_word(x: u8) -> bool {
    x.is_ascii_lowercase() || x.is_ascii_digit()
}
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
fn word_mask64(bytes: &[u8]) -> u64 {
    word_mask32(bytes) as u64 | ((word_mask32(&bytes[32..]) as u64) << 32)
}
fn append_positions(mut mask: u64, base: usize, out: &mut Vec<u32>) {
    while mask != 0 {
        let count = (mask.count_ones() as usize).min(8);
        let mut positions = [0u32; 8];
        // Eight unconditional bit extractions avoid a branch for each token boundary; only the valid prefix is appended.
        for position in &mut positions {
            *position = base as u32 + mask.trailing_zeros();
            mask &= mask.wrapping_sub(1);
        }
        out.extend_from_slice(&positions[..count]);
    }
}
#[rustfmt::skip]
pub fn token_positions(text: &[u8], starts: &mut Vec<u32>, ends: &mut Vec<u32>) {
    starts.clear(); ends.clear();
    let mut base = 0;
    let mut carry = 0u64;
    while base + 64 <= text.len() {
        let mask = word_mask64(&text[base..base + 64]);
        append_positions(mask & !((mask << 1) | carry), base, starts);
        append_positions(!mask & ((mask << 1) | carry), base, ends);
        carry = mask >> 63;
        base += 64;
    }
    let mut open = carry != 0;
    for (i, &byte) in text[base..].iter().enumerate() {
        let word = is_word(folded(byte));
        let at = (base + i) as u32;
        if word && !open { starts.push(at); }
        if !word && open { ends.push(at); }
        open = word;
    }
    if open { ends.push(text.len() as u32); }
}
fn hash_bytes(raw: &[u8]) -> u64 {
    raw.iter().fold(0xcbf29ce484222325, |h, &x| {
        (h ^ folded(x) as u64).wrapping_mul(0x100000001b3)
    })
}
#[derive(Clone, Copy, Default)]
struct ScanSlot {
    key0: u64,
    key1: u64,
    id_plus_one: usize,
    len: usize,
}
#[derive(Clone, Copy)]
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
        for (dst, &src) in bytes.iter_mut().zip(tail) {
            *dst = folded(src);
        }
        u64::from_le_bytes(bytes)
    }
}
fn scan_key(raw: &[u8]) -> ScanKey {
    let len = raw.len();
    if len <= 16 {
        let key0 = folded_word(raw, 0);
        let key1 = if len >= 8 {
            folded_word(raw, len - 8)
        } else {
            0
        };
        let hash = (key0 ^ key1.rotate_left(17) ^ len as u64).wrapping_mul(0x9e3779b185ebca87);
        ScanKey {
            key0,
            key1,
            hash: hash ^ (hash >> 32),
        }
    } else {
        let hash = hash_bytes(raw);
        ScanKey {
            key0: hash,
            key1: 0,
            hash,
        }
    }
}
pub struct ScanTable {
    slots: Vec<ScanSlot>,
    tags: Vec<u32>,
}
impl ScanTable {
    pub fn new(terms: &[Term]) -> Self {
        let mut capacity = 1usize;
        while capacity < terms.len().saturating_mul(3).div_ceil(2) {
            capacity *= 2;
        }
        let mut slots = vec![ScanSlot::default(); capacity];
        let mut tags = vec![0u32; capacity];
        for (id, term) in terms.iter().enumerate() {
            let key = scan_key(&term.bytes);
            let mut p = key.hash as usize & (capacity - 1);
            while slots[p].id_plus_one != 0 {
                p = (p + 1) & (capacity - 1);
            }
            slots[p] = ScanSlot {
                key0: key.key0,
                key1: key.key1,
                id_plus_one: id + 1,
                len: term.bytes.len(),
            };
            tags[p] = (key.hash >> 32) as u32 | 1;
        }
        Self { slots, tags }
    }
}
#[derive(Clone)]
struct HotTable {
    keys: Vec<(u64, u64)>,
    ids: Vec<u16>,
}
impl HotTable {
    fn new(terms: &[Term], ids: &[usize]) -> Self {
        let mut h = Self {
            keys: vec![(0, 0); 2048],
            ids: vec![u16::MAX; 2048],
        };
        for (i, &t) in ids.iter().enumerate() {
            let k = scan_key(&terms[t].bytes);
            let mut p = k.hash as usize & 2047;
            while h.ids[p] != u16::MAX {
                p = (p + 1) & 2047;
            }
            h.keys[p] = (k.key0, k.key1);
            h.ids[p] = i as u16;
        }
        h
    }
    fn lookup_key(&self, k: ScanKey, len: usize) -> Option<usize> {
        if len > 16 {
            return None;
        }
        let mut p = k.hash as usize & 2047;
        loop {
            let id = self.ids[p];
            if id == u16::MAX {
                return None;
            }
            if self.keys[p] == (k.key0, k.key1) {
                return Some(id as usize);
            }
            p = (p + 1) & 2047;
        }
    }
}
pub fn lookup(terms: &[Term], table: &ScanTable, raw: &[u8]) -> Option<usize> {
    if raw.is_empty() {
        return None;
    }
    lookup_key(terms, table, scan_key(raw), raw.len(), raw)
}
#[rustfmt::skip]
fn lookup_key(terms: &[Term], table: &ScanTable, key: ScanKey, len: usize, raw: &[u8]) -> Option<usize> {
    let mut p = key.hash as usize & (table.slots.len() - 1);
    let tag = (key.hash >> 32) as u32 | 1;
    loop {
        let found = table.tags[p];
        if found == 0 { return None; }
        if found == tag {
            let slot = table.slots[p];
            if slot.len == len && slot.key0 == key.key0 && slot.key1 == key.key1 {
                let id = slot.id_plus_one - 1;
                if len <= 16 || terms[id].bytes.iter().zip(raw).all(|(&a, &b)| a == folded(b)) { return Some(id); }
            }
        }
        p = (p + 1) & (table.slots.len() - 1);
    }
}
fn intern(raw: &[u8], terms: &mut Vec<Term>, slots: &mut Vec<u32>) -> usize {
    // Reuse the fast folded word hash; exact byte equality below keeps interning case-sensitive.
    let h = scan_key(raw).hash;
    let mut p = h as usize & (slots.len() - 1);
    loop {
        let v = slots[p];
        if v == 0 {
            let id = terms.len();
            terms.push(Term {
                bytes: raw.to_vec(),
                hash: h,
                df: 0,
                bitmap: false,
                offset: 0,
            });
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
#[rustfmt::skip]
pub fn parse_queries(data: &[u8]) -> (Vec<Term>, Vec<Query>, Vec<usize>, Vec<u8>) {
    let mut cap = 1usize;
    while cap < data.len() / 4 { cap *= 2; }
    let mut slots = vec![0u32; cap];
    let mut terms = Vec::new();
    let mut queries = Vec::new();
    let mut query_ids = Vec::new();
    let mut query_ops = Vec::new();
    for line in data.split(|&b| b == b'\n').filter(|x| !x.is_empty()) {
        let mut p = 0;
        while p < line.len() && line[p] != b' ' { p += 1; }
        let head = &line[..p];
        if head == b"BOOL" {
            let id_start = query_ids.len(); let op_start = query_ops.len();
            let mut q = p + 1;
            while q < line.len() {
                let s = q;
                while q < line.len() && line[q] != b' ' { q += 1; }
                query_ids.push(intern(&line[s..q], &mut terms, &mut slots));
                if q < line.len() {
                    q += 1;
                    let op = &line[q..];
                    if op.starts_with(b"OR") {
                        query_ops.push(1); q += 2;
                    } else if op.starts_with(b"NOT") {
                        query_ops.push(2); q += 3;
                    } else {
                        query_ops.push(0); q += 3;
                    }
                    if q < line.len() { q += 1; }
                }
            }
            queries.push(Query::Bool(id_start..query_ids.len(), op_start..query_ops.len()));
        } else {
            let s = p + 2;
            let e = line.len() - 1;
            let id_start = query_ids.len();
            for t in line[s..e].split(|&b| b == b' ') { query_ids.push(intern(t, &mut terms, &mut slots)); }
            queries.push(Query::Phrase(id_start..query_ids.len()));
        }
    }
    (terms, queries, query_ids, query_ops)
}
fn map_file(path: &Path) -> io::Result<(File, *mut u8, usize)> {
    use std::ffi::c_void;
    // SAFETY: this declaration matches the Linux mmap ABI; the call below uses a live descriptor and checked length.
    unsafe extern "C" {
        fn mmap(
            addr: *mut c_void,
            len: usize,
            prot: i32,
            flags: i32,
            fd: i32,
            off: i64,
        ) -> *mut c_void;
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
fn advise_will_need(address: usize, len: usize) {
    use std::ffi::c_void;
    // SAFETY: the declaration matches the Linux madvise ABI; the mapped address is retained by main until this thread joins.
    unsafe extern "C" {
        fn madvise(addr: *mut c_void, len: usize, advice: i32) -> i32;
    }
    // SAFETY: the address and length describe a live read-only mmap retained by main; MADV_WILLNEED does not modify it.
    unsafe {
        let _ = madvise(address as *mut c_void, len, 3);
    }
}
#[rustfmt::skip]
fn proc_mem() -> (u64, u64, u64) { let s=std::fs::read_to_string("/proc/self/status").unwrap_or_default(); let get=|name:&str| s.lines().find_map(|l|l.strip_prefix(name)?.split_whitespace().next()?.parse().ok()).unwrap_or(0); (get("RssAnon:"),get("VmHWM:"),get("RssFile:")) }
fn cgroup_mem() -> (String, String, String) {
    let read = |name: &str| {
        std::fs::read_to_string(format!("/sys/fs/cgroup/{name}"))
            .unwrap_or_else(|_| "unavailable".into())
    };
    let peak = read("memory.peak").trim().to_owned();
    let current = read("memory.current").trim().to_owned();
    let stat = read("memory.stat");
    let field = |name: &str| {
        stat.split_whitespace()
            .collect::<Vec<_>>()
            .windows(2)
            .find(|x| x[0] == name)
            .map_or("unavailable", |x| x[1])
    };
    (
        peak,
        current,
        format!(
            "anon={} file={} kernel={} shmem={}",
            field("anon"),
            field("file"),
            field("kernel"),
            field("shmem")
        ),
    )
}
#[repr(C)]
struct RUsage([i64; 18]);
#[rustfmt::skip]
fn fault_snapshot() -> (i64, i64) { unsafe extern "C" { fn getrusage(who:i32, usage:*mut RUsage)->i32; } let mut r=RUsage([0;18]); /* SAFETY: RUsage is the Linux x86-64 rusage field layout and the output is a valid writable struct. */ unsafe { let _=getrusage(0,&mut r); } (r.0[8],r.0[9]) }
fn merge(a: &[u32], b: &[u32], op: u8) -> Vec<u32> {
    let mut out = Vec::with_capacity(if op == 1 {
        a.len() + b.len()
    } else {
        a.len().min(b.len())
    });
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
fn as_list(s: Set<'_>, n: usize) -> Vec<u32> {
    match s {
        Set::List(v) => v.into_owned(),
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
#[rustfmt::skip]
fn bitmap_op<'a>(a: Set<'a>, b: Set<'a>, op: u8, n: usize, scratch: &mut Vec<u64>) -> Set<'a> {
    let words = (n + 63) / 64;
    if matches!((&a, &b), (Set::List(_), Set::List(_))) {
        let (Set::List(x), Set::List(y)) = (a, b) else { unreachable!() };
        return Set::List(Cow::Owned(merge(x.as_ref(), y.as_ref(), op)));
    }
    let (mut out, left_list) = match a {
        Set::Bits(Cow::Owned(v)) => (v, None),
        Set::Bits(Cow::Borrowed(v)) => { let mut out = std::mem::take(scratch); out.resize(words, 0); out.copy_from_slice(v); (out, None) }
        Set::List(v) => { let mut out = std::mem::take(scratch); out.resize(words, 0); out.fill(0); (out, Some(v.into_owned())) }
    };
    match (left_list, b) {
        (Some(left), Set::Bits(right)) => { let right = right.as_ref(); match op {
            0 => for id in left { let i = id as usize; if right[i / 64] & (1u64 << (i % 64)) != 0 { out[i / 64] |= 1u64 << (i % 64); } },
            1 => { out.copy_from_slice(right); for id in left { let i = id as usize; out[i / 64] |= 1u64 << (i % 64); } },
            _ => for id in left { let i = id as usize; if right[i / 64] & (1u64 << (i % 64)) == 0 { out[i / 64] |= 1u64 << (i % 64); } },
        }},
        (None, Set::Bits(right)) => {
            let right = right.as_ref();
            for i in 0..words { out[i] = match op { 0 => out[i] & right[i], 1 => out[i] | right[i], _ => out[i] & !right[i] }; }
        }
        (None, Set::List(right)) => {
            if op == 0 {
                let mut at = 0;
                for word in 0..words {
                    let old = out[word]; let mut kept = 0;
                    while at < right.len() && right[at] as usize / 64 == word {
                        let bit = 1u64 << (right[at] % 64);
                        if old & bit != 0 { kept |= bit; }
                        at += 1;
                    }
                    out[word] = kept;
                }
            } else {
                for &id in right.iter() {
                    let i = id as usize; let mask = 1u64 << (i % 64);
                    if op == 1 { out[i / 64] |= mask; } else { out[i / 64] &= !mask; }
                }
            }
        }
        (Some(_), Set::List(_)) => unreachable!("the list/list case returned above"),
    }
    Set::Bits(Cow::Owned(out))
}
impl Postings {
    fn list(&self, t: usize, terms: &[Term]) -> Vec<u32> {
        let mut out = Vec::with_capacity(terms[t].df);
        for run in &self.runs {
            let start = run.term_block_starts[t] as usize;
            let end = run.term_block_starts[t + 1] as usize;
            for &offset in &run.block_offsets[start..end] {
                let offset = offset as usize;
                let block = u32::from_le_bytes(run.data[offset..offset + 4].try_into().unwrap());
                let count = run.data[offset + 4] as usize;
                debug_assert_eq!(run.data[offset + 5], 7);
                let payload = &run.data[offset + 6..];
                for i in 0..count {
                    let bit = i * 7;
                    let byte = bit / 8;
                    let shift = bit % 8;
                    let mut value = (payload[byte] as u16) >> shift;
                    if shift > 1 {
                        value |= (payload[byte + 1] as u16) << (8 - shift);
                    }
                    out.push(block * 128 + (value as u32 & 0x7f));
                }
            }
        }
        out
    }

    fn contains(&self, t: usize, doc: u32) -> bool {
        let run_index = match self
            .bases
            .iter()
            .zip(&self.ends)
            .position(|(&s, &e)| s <= doc && doc < e)
        {
            Some(i) => i,
            None => return false,
        };
        let run = &self.runs[run_index];
        let start = run.term_block_starts[t] as usize;
        let end = run.term_block_starts[t + 1] as usize;
        let target = doc >> 7;
        let (mut lo, mut hi) = (start, end);
        while lo < hi {
            let mid = (lo + hi) / 2;
            let offset = run.block_offsets[mid] as usize;
            let block = u32::from_le_bytes(run.data[offset..offset + 4].try_into().unwrap());
            if block < target {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        if lo == end {
            return false;
        }
        let offset = run.block_offsets[lo] as usize;
        if u32::from_le_bytes(run.data[offset..offset + 4].try_into().unwrap()) != target {
            return false;
        }
        let count = run.data[offset + 4] as usize;
        let delta = (doc & 127) as u16;
        let payload = &run.data[offset + 6..];
        for i in 0..count {
            let bit = i * 7;
            let byte = bit / 8;
            let shift = bit % 8;
            let mut value = (payload[byte] as u16) >> shift;
            if shift > 1 {
                value |= (payload[byte + 1] as u16) << (8 - shift);
            }
            let found = value & 0x7f;
            if found == delta {
                return true;
            }
            if found > delta {
                return false;
            }
        }
        false
    }
}

fn finish_compressed_block(run: &mut CompressedRun, block: u32, deltas: &[u8]) {
    let offset = run.data.len() as u32;
    run.block_offsets.push(offset);
    run.data.extend_from_slice(&block.to_le_bytes());
    run.data.push(deltas.len() as u8);
    run.data.push(7);
    let mut packed = vec![0u8; (deltas.len() * 7).div_ceil(8)];
    for (i, &delta) in deltas.iter().enumerate() {
        let bit = i * 7;
        let byte = bit / 8;
        let shift = bit % 8;
        let value = (delta as u16) << shift;
        packed[byte] |= value as u8;
        if byte + 1 < packed.len() {
            packed[byte + 1] |= (value >> 8) as u8;
        }
    }
    run.data.extend_from_slice(&packed);
}

fn encode_compressed<I: Iterator<Item = (usize, u32)>>(
    pairs: I,
    terms_len: usize,
) -> CompressedRun {
    let mut run = CompressedRun {
        data: Vec::new(),
        block_offsets: Vec::new(),
        term_block_starts: vec![0; terms_len + 1],
    };
    let mut next_term = 0;
    let mut active: Option<(usize, u32)> = None;
    let mut deltas = Vec::with_capacity(128);
    for (term, doc) in pairs {
        let block = doc >> 7;
        if active.is_some_and(|(old_term, old_block)| old_term != term || old_block != block) {
            finish_compressed_block(&mut run, active.unwrap().1, &deltas);
            deltas.clear();
            active = None;
        }
        while next_term < term {
            run.term_block_starts[next_term + 1] = run.block_offsets.len() as u32;
            next_term += 1;
        }
        if active.is_none() {
            active = Some((term, block));
        }
        deltas.push((doc & 127) as u8);
    }
    if let Some((_, block)) = active {
        finish_compressed_block(&mut run, block, &deltas);
    }
    while next_term < terms_len {
        run.term_block_starts[next_term + 1] = run.block_offsets.len() as u32;
        next_term += 1;
    }
    run
}

fn compress_pairs(pairs: PairBuffer, doc_base: u32, terms_len: usize) -> CompressedRun {
    match pairs {
        PairBuffer::Packed(pairs) => encode_compressed(
            pairs
                .into_iter()
                .map(|pair| ((pair >> 15) as usize, doc_base + (pair & 0x7fff))),
            terms_len,
        ),
        PairBuffer::Full(pairs) => encode_compressed(
            pairs.into_iter().map(|(term, doc)| (term as usize, doc)),
            terms_len,
        ),
    }
}
fn term_set<'a>(
    t: usize,
    terms: &[Term],
    posts: &'a Postings,
    bits: &'a [u64],
    n: usize,
) -> Set<'a> {
    let x = &terms[t];
    if x.bitmap {
        let w = (n + 63) / 64;
        Set::Bits(Cow::Borrowed(&bits[x.offset..x.offset + w]))
    } else {
        Set::List(Cow::Owned(posts.list(t, terms)))
    }
}
fn contains_doc(t: usize, doc: u32, terms: &[Term], posts: &Postings, bits: &[u64]) -> bool {
    let term = &terms[t];
    if term.bitmap {
        bits[term.offset + doc as usize / 64] & (1u64 << (doc % 64)) != 0
    } else {
        posts.contains(t, doc)
    }
}
fn same_token(raw: &[u8], term: &[u8]) -> bool {
    raw.len() == term.len() && raw.iter().zip(term).all(|(&a, &b)| folded(a) == folded(b))
}
#[rustfmt::skip]
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
#[rustfmt::skip]
pub fn phrase_in_doc(data: &[u8], ids: &[usize], anchor: usize, terms: &[Term]) -> bool {
    let first = folded(terms[anchor].bytes[0]);
    let mut anchor_positions = [0usize; 8]; let mut count = 0;
    for (i, &id) in ids.iter().enumerate() { if id == anchor { anchor_positions[count] = i; count += 1; } }
    let mut base = 0;
    while base < data.len() {
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
fn push_decimal(out: &mut Vec<u8>, mut value: u64) {
    let mut buf = [0u8; 20];
    let mut p = buf.len();
    loop {
        p -= 1;
        buf[p] = b'0' + (value % 10) as u8;
        value /= 10;
        if value == 0 {
            break;
        }
    }
    out.extend_from_slice(&buf[p..]);
}
#[rustfmt::skip]
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
    push_decimal(out, count as u64); out.push(b' '); push_decimal(out, sum);
    for id in first { out.push(b' '); push_decimal(out, id as u64); }
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
fn decimal_digits(mut id: usize) -> usize {
    let mut n = 1;
    while id >= 10 {
        id /= 10;
        n += 1;
    }
    n
}
fn doc_count(docs: &[u8]) -> usize {
    if docs.is_empty() {
        return 0;
    }
    let end = docs.len() - usize::from(docs.last() == Some(&b'\n'));
    let start = docs[..end]
        .iter()
        .rposition(|&b| b == b'\n')
        .map_or(0, |p| p + 1);
    document_id(&docs[start..end]).0 + 1
}
#[rustfmt::skip]
fn sample_hot(docs: &[u8], terms: &[Term], table: &ScanTable) -> Vec<usize> {
    let mut counts = vec![0u32; terms.len()];
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
    let local_counts: Vec<Vec<u32>> = std::thread::scope(|scope| {
        let handles: Vec<_> = ranges.iter().map(|&(s, e)| scope.spawn(move || {
            let mut local_counts = vec![0u32; terms.len()];
            let mut local_last = vec![u32::MAX; terms.len()];
            let (mut starts, mut ends) = (Vec::new(), Vec::new());
            let mut p = s;
            while p < e {
                let end = docs[p..e].iter().position(|&b| b == b'\n').map_or(e, |x| p + x);
                let line = &docs[p..end]; let id = document_id(line).0 as u32;
                if let Some(tab) = line.iter().position(|&b| b == b'\t') {
                    let text = &line[tab + 1..]; token_positions(text, &mut starts, &mut ends);
                    for (&a, &b) in starts.iter().zip(&ends) {
                        if let Some(t) = lookup(terms, table, &text[a as usize..b as usize]) {
                            if local_last[t] != id { local_last[t] = id; local_counts[t] += 1; }
                        }
                    }
                }
                p = end + 1;
            }
            local_counts
        })).collect();
        handles.into_iter().map(|h| h.join().expect("hot sample worker panicked")).collect()
    });
    for local in local_counts { for (total, count) in counts.iter_mut().zip(local) { *total += count; } }
    let mut ids: Vec<_> = (0..terms.len()).filter(|&t| counts[t] >= 2 && terms[t].bytes.len() <= 16).collect();
    ids.sort_unstable_by_key(|&t| (std::cmp::Reverse(counts[t]), t));
    ids.truncate(1024);
    ids
}
struct WorkerScan {
    counts: Vec<u32>,
    pairs: PairBuffer,
    doc_base: u32,
    rows: usize,
    elapsed_us: u128,
}
enum PairBuffer {
    Packed(Vec<u32>),
    Full(Vec<(u32, u32)>),
}
#[rustfmt::skip]
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
    line_ptr: usize,
) -> WorkerScan {
    let mut state = vec![(u32::MAX, 0u32); terms.len()];
    let mut hot_last = vec![u32::MAX; hot_len];
    let (mut starts, mut ends, mut keys) = (Vec::new(), Vec::new(), Vec::new());
    // A packed pair uses 17 term bits and 15 doc-offset bits; overflow switches to exact full-width storage.
    let mut packed = Vec::with_capacity((end - start) / 24);
    let mut full = (terms.len() >= (1 << 17)).then(|| Vec::with_capacity((end - start) / 24));
    let mut doc_base = None;
    let (mut pos, mut rows) = (start, 0);
    while pos < end {
        let end_line = docs[pos..end].iter().position(|&b| b == b'\n').map_or(end, |x| pos + x);
        let line = &docs[pos..end_line];
        let (raw_id, tab) = document_id(line);
        let id = raw_id as u32;
        let base = *doc_base.get_or_insert(id);
        if full.is_none() && (id < base || id - base >= (1 << 15)) {
            let old = std::mem::take(&mut packed);
            full = Some(old.into_iter().map(|x| ((x >> 15), base + (x & 0x7fff))).collect());
        }
        // SAFETY: dense document IDs are unique and each worker owns a disjoint newline-aligned range.
        unsafe {
            (line_ptr as *mut u32).add(id as usize).write(pos as u32);
        }
        let text = &line[tab + 1..];
        token_positions(text, &mut starts, &mut ends);
        keys.clear();
        for (&s, &e) in starts.iter().zip(&ends) { keys.push(scan_key(&text[s as usize..e as usize])); }
        for i in 0..keys.len() {
            let s = starts[i] as usize; let e = ends[i] as usize; let raw = &text[s..e]; let key = keys[i];
            if i + 12 < keys.len() {
                let prefetch = keys[i + 12].hash as usize & (table.tags.len() - 1);
                // SAFETY: prefetch points to a valid tag in the live table; the intrinsic does not dereference it in Rust.
                unsafe { std::arch::x86_64::_mm_prefetch(table.tags.as_ptr().add(prefetch).cast(), std::arch::x86_64::_MM_HINT_T0); }
            }
            if let Some(h) = hot.lookup_key(key, raw.len()) {
                if hot_last[h] != id {
                    hot_last[h] = id;
                    bits[h * words + id as usize / 64].fetch_or(1 << (id % 64), Ordering::Relaxed);
                }
            } else if let Some(t) = lookup_key(terms, table, key, raw.len(), raw) {
                let h = term_hot[t];
                if h != u16::MAX {
                    let h = h as usize;
                    if hot_last[h] != id {
                        hot_last[h] = id;
                        bits[h * words + id as usize / 64].fetch_or(1 << (id % 64), Ordering::Relaxed);
                    }
                } else if state[t].0 != id {
                    state[t].0 = id;
                    state[t].1 += 1;
                    if let Some(pairs) = &mut full { pairs.push((t as u32, id)); }
                    else { packed.push(((t as u32) << 15) | (id - base)); }
                }
            }
        }
        rows += 1;
        pos = end_line + 1;
    }
    let counts = state.into_iter().map(|x| x.1).collect();
    let pairs = full.map_or(PairBuffer::Packed(packed), PairBuffer::Full);
    WorkerScan { counts, pairs, doc_base: doc_base.unwrap_or(0), rows, elapsed_us: 0 }
}
#[rustfmt::skip]
fn radix_sort_pairs(pairs: &mut PairBuffer) {
    let PairBuffer::Full(pairs) = pairs else {
        let PairBuffer::Packed(pairs) = pairs else { unreachable!() };
        let mut scratch = vec![0u32; pairs.len()];
        for shift in [0, 16] {
            let mut offsets = vec![0usize; 1 << 16];
            for &pair in pairs.iter() { offsets[((pair >> (15 + shift)) & 0xffff) as usize] += 1; }
            let mut next = 0;
            for count in &mut offsets { let n = *count; *count = next; next += n; }
            for &pair in pairs.iter() { let slot = &mut offsets[((pair >> (15 + shift)) & 0xffff) as usize]; scratch[*slot] = pair; *slot += 1; }
            std::mem::swap(pairs, &mut scratch);
        }
        return;
    };
    let mut scratch = vec![(0u32, 0u32); pairs.len()];
    for shift in [0, 16] {
        let mut offsets = vec![0usize; 1 << 16];
        for &(term, _) in pairs.iter() { offsets[((term >> shift) & 0xffff) as usize] += 1; }
        let mut next = 0;
        for count in &mut offsets { let n = *count; *count = next; next += n; }
        for &pair in pairs.iter() {
            let term = pair.0;
            let slot = &mut offsets[((term >> shift) & 0xffff) as usize];
            scratch[*slot] = pair; *slot += 1;
        }
        std::mem::swap(pairs, &mut scratch);
    }
}
#[rustfmt::skip]
fn eval_query(
    q: &Query,
    query_ids: &[usize],
    query_ops: &[u8],
    terms: &[Term],
    posts: &Postings,
    bits: &[u64],
    docs: &[u8],
    line_starts: &[u32],
    n: usize,
    bitmap_scratch: &mut Vec<u64>,
) -> Vec<u8> {
    let mut out = Vec::with_capacity(64);
    match q {
        Query::Bool(id_range, op_range) => {
            let ids = &query_ids[id_range.clone()]; let ops = &query_ops[op_range.clone()];
            let mut r = term_set(ids[0], terms, posts, bits, n);
            for (i, &t) in ids.iter().enumerate().skip(1) {
                r = bitmap_op(r, term_set(t, terms, posts, bits, n), ops[i - 1], n, bitmap_scratch);
            }
            match r {
                Set::Bits(Cow::Owned(words)) => {
                    output_set(Set::Bits(Cow::Borrowed(&words)), n, &mut out);
                    *bitmap_scratch = words;
                }
                other => output_set(other, n, &mut out),
            }
        }
        Query::Phrase(range) => {
            let ids = &query_ids[range.clone()];
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
                let line_start = line_starts[d as usize] as usize;
                let next_line = line_starts[d as usize + 1] as usize;
                let start = line_start + decimal_digits(d as usize) + 1;
                let end = next_line - usize::from(next_line > line_start && docs[next_line - 1] == b'\n');
                if phrase_in_doc(&docs[start..end], ids, anchor, terms) {
                    found.push(d);
                }
            }
            let sum: u64 = found.iter().map(|&x| x as u64).sum();
            push_decimal(&mut out, found.len() as u64); out.push(b' '); push_decimal(&mut out, sum);
            for id in found.iter().take(10) { out.push(b' '); push_decimal(&mut out, *id as u64); }
            out.push(b'\n');
        }
    }
    out
}
#[rustfmt::skip]
fn main() -> io::Result<()> {
    let timing = std::env::var_os("SD_TIMING").is_some(); let total = Instant::now(); let mut lap = total; let mut phases = [0u128; 7];
    macro_rules! snap { ($p:expr) => { if timing { let (a,h,f)=proc_mem(); eprintln!("SD_MEM phase={} anon={} hwm={} file={}",$p,a,h,f); } }; }
    macro_rules! tick { ($i:expr) => { if timing { phases[$i] = lap.elapsed().as_micros(); lap = Instant::now(); snap!($i); } }; }
    let args: Vec<String> = std::env::args().collect();
    let root = args.get(1).map_or("/data", String::as_str); let output = args.get(2).map_or("/out", String::as_str);
    let (_file, ptr, len) = map_file(Path::new(&format!("{root}/docs.txt")))?;
    // SAFETY: the read-only slice covers the live mmap and does not outlive it.
    let docs = unsafe { std::slice::from_raw_parts(ptr, len) };
    let mapping = ptr as usize;
    let advise = std::thread::spawn(move || advise_will_need(mapping, len));
    let read_start = Instant::now(); let qdata = std::fs::read(format!("{root}/queries.txt"))?; let query_read = read_start.elapsed().as_micros();
    let parse_start = Instant::now(); let (mut terms, queries, query_ids, query_ops) = parse_queries(&qdata); let query_parse = parse_start.elapsed().as_micros();
    advise.join().expect("madvise worker panicked");
    tick!(0); let table = ScanTable::new(&terms); let hot_ids = sample_hot(docs, &terms, &table); tick!(1);
    let n = doc_count(docs);
    let mut term_hot = vec![u16::MAX; terms.len()];
    for (h, &t) in hot_ids.iter().enumerate() { term_hot[t] = h as u16; terms[t].bitmap = true; terms[t].offset = h * n.div_ceil(64); }
    let words = n.div_ceil(64); let hot_len = hot_ids.len();
    let bitmap_words = vec![0u64; hot_len * words];
    let mut line_starts = vec![0u32; n + 1]; let line_ptr = line_starts.as_mut_ptr() as usize;
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
    let scan_faults_before = if timing { fault_snapshot() } else { (0, 0) };
    let mut scans: Vec<WorkerScan> = {
        // SAFETY: AtomicU64 has the same size and alignment as u64; this zeroed storage is only accessed atomically until workers join.
        let bitmaps = unsafe { std::slice::from_raw_parts(bitmap_words.as_ptr().cast::<AtomicU64>(), bitmap_words.len()) };
        let scans: Vec<_> = std::thread::scope(|scope| {
            let handles: Vec<_> = chunks.iter().enumerate().map(|(w, &(s, e))| {
                let (ts, st, ht, th, bm) = (&terms, &table, &hot_tables[w], &term_hot, bitmaps);
                scope.spawn(move || { let start=Instant::now(); let mut x=scan_onepass(docs,s,e,ts,st,ht,th,hot_len,words,bm,line_ptr); x.elapsed_us=start.elapsed().as_micros(); x })
            }).collect();
            handles.into_iter().map(|h| h.join().expect("one-pass worker panicked")).collect()
        });
        debug_assert_eq!(scans.iter().map(|x| x.rows).sum::<usize>(), n);
    line_starts[n] = len as u32;
        snap!("pair_collection");
        for (t, term) in terms.iter_mut().enumerate() {
            term.df = if term.bitmap {
                bitmaps[term.offset..term.offset + words].iter().map(|x| x.load(Ordering::Relaxed).count_ones() as usize).sum()
            } else { scans.iter().map(|x| x.counts[t] as usize).sum() };
        }
        scans
    };
    // Sort workers one at a time so the temporary radix arrays do not multiply peak RSS by the worker count.
    for scan in &mut scans { radix_sort_pairs(&mut scan.pairs); }
    snap!("radix_done");
    let terms_len = terms.len();
    let mut runs = Vec::with_capacity(scans.len());
    let mut bases = Vec::with_capacity(scans.len());
    let mut ends = Vec::with_capacity(scans.len());
    snap!("scatter_start");
    for scan in &mut scans {
        bases.push(scan.doc_base);
        ends.push(scan.doc_base + scan.rows as u32);
        let pairs = std::mem::replace(&mut scan.pairs, PairBuffer::Packed(Vec::new()));
        runs.push(compress_pairs(pairs, scan.doc_base, terms_len));
    }
    let scan_faults_after = if timing { fault_snapshot() } else { (0, 0) };
    let scan_faults = (scan_faults_after.0-scan_faults_before.0, scan_faults_after.1-scan_faults_before.1);
    let worker_times: Vec<u128> = scans.iter().map(|x|x.elapsed_us).collect();
    if timing { let bytes: usize = runs.iter().map(|r| r.data.len() + r.block_offsets.len() * 4 + r.term_block_starts.len() * 4).sum(); eprintln!("SD_COMPRESSED bytes={} blocks={}", bytes, runs.iter().map(|r| r.block_offsets.len()).sum::<usize>()); }
    snap!("pairs_pre_drop"); drop(scans); tick!(4);
    let postings = Postings { runs, bases, ends };
    let bits = &bitmap_words[..];
    let next = AtomicUsize::new(0); const BLOCK: usize = 64; let qlen = queries.len();
    let out = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..workers.min(qlen.max(1))).map(|_| {
            let (ts, qi, qo, ps, bm, starts) = (&terms, &query_ids, &query_ops, &postings, bits, &line_starts); let next = &next; let qs = &queries;
            scope.spawn(move || {
                let mut blocks = Vec::new(); let mut bitmap_scratch = Vec::new(); loop {
                    let s = next.fetch_add(BLOCK, Ordering::Relaxed); if s >= qlen { break; }
                    let mut part = Vec::new();
                    for q in &qs[s..(s + BLOCK).min(qlen)] { part.extend_from_slice(&eval_query(q, qi, qo, ts, ps, bm, docs, starts, n, &mut bitmap_scratch)); }
                    blocks.push((s / BLOCK, part));
                } blocks
            })
        }).collect();
        let mut blocks: Vec<_> = handles.into_iter().flat_map(|h| h.join().expect("query worker panicked")).collect();
        blocks.sort_unstable_by_key(|x| x.0); blocks.into_iter().flat_map(|x| x.1).collect::<Vec<u8>>()
    });
    tick!(5); snap!("query_complete"); std::fs::create_dir_all(output)?; std::fs::write(Path::new(output).join("results.txt"), out)?;
    if timing { phases[6] = lap.elapsed().as_micros(); let rss=proc_mem(); eprintln!("SD_TIMING read={} query_parse={} parse={} table={} scan_a={} setup={} scan_b={} queries={} output={} total={} rss_anon_kb={} vmhwm_kb={} rss_file_kb={} scan_minor={} scan_major={} workers={:?}",query_read,query_parse,phases[0],phases[1],phases[2],phases[3],phases[4],phases[5],phases[6],total.elapsed().as_micros(),rss.0,rss.1,rss.2,scan_faults.0,scan_faults.1,worker_times); let (peak,current,stat)=cgroup_mem(); eprintln!("SD_CGROUP memory_peak={} memory_current={} {}",peak,current,stat); }
    std::process::exit(0)
}
