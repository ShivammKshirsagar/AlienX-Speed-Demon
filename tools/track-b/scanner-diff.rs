#[path = "../../track-b/src/main.rs"]
mod solution;

use std::fs;

fn scalar_tokens<F: FnMut(&[u8])>(data: &[u8], mut visit: F) {
    let mut p = 0;
    while p < data.len() {
        while p < data.len() && !data[p].is_ascii_alphanumeric() { p += 1; }
        let start = p;
        while p < data.len() && data[p].is_ascii_alphanumeric() { p += 1; }
        if p > start { visit(&data[start..p]); }
    }
}

fn ids_for_line(line: &[u8], terms: &[solution::Term], table: &solution::ScanTable, scalar: bool) -> Vec<usize> {
    let body = line.iter().position(|&b| b == b'\t').map_or(&[][..], |p| &line[p + 1..]);
    let mut ids = Vec::new();
    let mut push = |token: &[u8]| ids.push(solution::lookup(terms, table, token).unwrap_or(usize::MAX));
    if scalar {
        scalar_tokens(body, &mut push);
    } else {
        solution::scan_tokens(body, &mut push);
    }
    ids
}

fn check_equal(bytes: &[u8], chunk_bytes: usize) {
    let mut scalar = Vec::new();
    scalar_tokens(bytes, |t| scalar.push(t.to_vec()));
    #[cfg(target_feature = "avx2")]
    {
        let mut fast = Vec::new();
        solution::scan_tokens_avx2_chunks(bytes, chunk_bytes, |t| fast.push(t.to_vec()));
        assert_eq!(scalar, fast, "token boundary mismatch in synthetic input");
    }
    #[cfg(not(target_feature = "avx2"))]
    panic!("scanner differential must be compiled with AVX2 enabled");
}

fn main() {
    let root = std::env::args().nth(1).expect("sample dataset path");
    let queries = fs::read(format!("{root}/queries.txt")).unwrap();
    let (terms, _) = solution::parse_queries(&queries);
    let table = solution::ScanTable::new(&terms);
    let docs = fs::read(format!("{root}/docs.txt")).unwrap();
    let mut checked = 0usize;
    for line in docs.split(|&b| b == b'\n') {
        if line.is_empty() {
            continue;
        }
        let old = ids_for_line(line, &terms, &table, true);
        let new = ids_for_line(line, &terms, &table, false);
        assert_eq!(old, new, "document token IDs differ at document line {checked}");
        checked += 1;
    }

    let mut synthetic = Vec::new();
    for len in 1..=40 {
        synthetic.extend(std::iter::repeat_n(b'a', len));
        synthetic.push(b'!');
    }
    synthetic.extend_from_slice(&[b'a'; 31]);
    synthetic.extend_from_slice(b"BCdefghijklmnopqrstuvwxyz0123456789");
    synthetic.extend_from_slice(&[b'!'; 35]);
    synthetic.extend_from_slice(b"abcdefghijklmnopqrstuvwxyz0123456789abcd");
    synthetic.extend_from_slice(b"ab\x80cd\t9\n\n77\t!!!\n88\tZ9\n");
    check_equal(&synthetic, 37);

    for line in [b"0\t".as_slice(), b"1\t!!!", b"2\ta", b"3\tABC\x80def"] {
        let old = ids_for_line(line, &terms, &table, true);
        let new = ids_for_line(line, &terms, &table, false);
        assert_eq!(old, new, "synthetic document field/token mismatch");
    }
    println!("scanner differential passed: {checked} sample documents and synthetic edge cases");
}
