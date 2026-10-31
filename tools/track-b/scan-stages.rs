#![allow(dead_code)]

#[path = "../../track-b/src/main.rs"]
mod solution;

use std::{io::Read, mem::size_of, time::Instant};

#[repr(C)]
struct RUsage([i64; 18]);
unsafe extern "C" { fn getrusage(who: i32, usage: *mut RUsage) -> i32; }

fn faults() -> (i64, i64) {
    let mut usage = RUsage([0; 18]);
    // SAFETY: RUsage matches Linux x86-64 struct rusage size and layout; the pointer is writable and lives through the call.
    assert_eq!(unsafe { getrusage(0, &mut usage) }, 0);
    (usage.0[8], usage.0[9])
}

fn measure(stage: usize, docs: &[u8], terms: &[solution::Term], table: &solution::ScanTable) -> (u128, u64, u64, usize, i64, i64) {
    let mut last = vec![u32::MAX; terms.len()];
    let mut counts = vec![0u32; terms.len()];
    let mut pairs = Vec::new();
    let (mut starts, mut ends) = (Vec::new(), Vec::new());
    let (mut tokens, mut hits, mut checksum) = (0u64, 0u64, 0u64);
    let before = faults();
    let begin = Instant::now();
    for line in docs.split(|&b| b == b'\n') {
        if line.is_empty() { continue; }
        let tab = line.iter().position(|&b| b == b'\t').unwrap();
        let text = &line[tab + 1..];
        let id = line[..tab].iter().fold(0u32, |n, &b| n * 10 + (b - b'0') as u32);
        solution::token_positions(text, &mut starts, &mut ends);
        tokens += starts.len() as u64;
        if stage == 0 { checksum = checksum.wrapping_add(starts.last().copied().unwrap_or(0) as u64); continue; }
        for (&s, &e) in starts.iter().zip(&ends) {
            let token = &text[s as usize..e as usize];
            if let Some(t) = solution::lookup(terms, table, token) {
                hits += 1;
                checksum = checksum.wrapping_add(t as u64);
                if stage >= 2 && last[t] != id {
                    last[t] = id;
                    counts[t] += 1;
                    if stage >= 3 { pairs.push((t as u32, id)); }
                }
            }
        }
    }
    if stage >= 4 {
        let mut scratch = vec![(0u32, 0u32); pairs.len()];
        for shift in [0, 16] {
            let mut nexts = vec![0usize; 1 << 16];
            for &(term, _) in &pairs { nexts[((term >> shift) & 0xffff) as usize] += 1; }
            let mut next = 0;
            for n in &mut nexts { let count = *n; *n = next; next += count; }
            for &pair in &pairs { let slot = &mut nexts[((pair.0 >> shift) & 0xffff) as usize]; scratch[*slot] = pair; *slot += 1; }
            std::mem::swap(&mut pairs, &mut scratch);
        }
        let mut offsets = vec![0usize; terms.len()];
        let mut total = 0usize;
        for (i, &count) in counts.iter().enumerate() { offsets[i] = total; total += count as usize; }
        let mut postings = vec![0u32; total];
        let mut cursor = vec![0usize; terms.len()];
        for &(t, id) in &pairs { let t = t as usize; postings[offsets[t] + cursor[t]] = id; cursor[t] += 1; }
        checksum = checksum.wrapping_add(postings.last().copied().unwrap_or(0) as u64);
    }
    let elapsed = begin.elapsed().as_nanos();
    let after = faults();
    std::hint::black_box(checksum);
    (elapsed, tokens, hits, pairs.len(), after.0 - before.0, after.1 - before.1)
}

fn main() {
    let mut file = std::fs::File::open("/data/docs.txt").unwrap();
    let target = 16 * 1024 * 1024;
    let mut docs = vec![0u8; target + 65_536];
    file.read_exact(&mut docs).unwrap();
    let cut = target + docs[target..].iter().position(|&b| b == b'\n').unwrap() + 1;
    docs.truncate(cut);
    let q = std::fs::read("/data/queries.txt").unwrap();
    let (terms, _, _, _) = solution::parse_queries(&q);
    let table = solution::ScanTable::new(&terms);
    let mut rows = Vec::new();
    for stage in 0..5 { rows.push(measure(stage, &docs, &terms, &table)); }
    let tokens = rows[0].1;
    for (stage, row) in rows.iter().enumerate() {
        println!("stage={stage} ns_per_token={:.2} tokens={} query_hit_rate={:.3} pairs={} minor_faults={} major_faults={}", row.0 as f64 / tokens as f64, row.1, row.2 as f64 / tokens as f64, row.3, row.4, row.5);
    }
    let mut cap = 1usize;
    while cap < terms.len().saturating_mul(3).div_ceil(2) { cap *= 2; }
    let term_table = cap * (2 * size_of::<u64>() + 2 * size_of::<usize>() + size_of::<u32>());
    println!("arrays_bytes term_table_plus_tags={} term_bytes={} last_doc={} counters={} pairs_stage4={} offsets_single_worker={} offsets_12_workers={}", term_table, solution::benchmark_term_bytes(&terms), terms.len() * 4, terms.len() * 4, rows[3].3 * 8, terms.len() * 4, terms.len() * 12 * 4);
}
