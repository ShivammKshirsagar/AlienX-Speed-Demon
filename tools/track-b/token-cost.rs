#[path = "../../track-b/src/main.rs"]
mod solution;

use std::io::Read;
use std::time::Instant;

fn main() {
    let mut file = std::fs::File::open("/data/docs.txt").unwrap();
    let target = 16 * 1024 * 1024;
    let mut docs = vec![0u8; target + 65_536];
    file.read_exact(&mut docs).unwrap();
    let cut = target + docs[target..].iter().position(|&b| b == b'\n').unwrap() + 1;
    docs.truncate(cut);
    let queries = std::fs::read("/data/queries.txt").unwrap();
    let (terms, _, _, _) = solution::parse_queries(&queries);
    let table = solution::ScanTable::new(&terms);
    let mut tokens = 0u64;
    let mut checksum = 0u64;
    let start = Instant::now();
    let (mut starts, mut ends) = (Vec::new(), Vec::new());
    for line in docs.split(|&b| b == b'\n') {
        if line.is_empty() {
            continue;
        }
        let tab = line.iter().position(|&b| b == b'\t').unwrap(); let text = &line[tab + 1..];
        solution::token_positions(text, &mut starts, &mut ends);
        for (&s, &e) in starts.iter().zip(&ends) {
            let token = &text[s as usize..e as usize];
            tokens += 1;
            checksum = checksum.wrapping_add(solution::lookup(&terms, &table, token).unwrap_or(usize::MAX) as u64);
        }
    }
    let elapsed = start.elapsed().as_nanos();
    std::hint::black_box(checksum);
    println!("slice_bytes={} tokens={} tokenize_plus_probe_ns_per_token={:.2}", cut, tokens, elapsed as f64 / tokens as f64);
}
