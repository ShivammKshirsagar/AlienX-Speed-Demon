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

fn old_match(data: &[u8], ids: &[usize], terms: &[solution::Term], table: &solution::ScanTable) -> bool {
    let mut window = [usize::MAX; 8];
    let mut count = 0usize;
    let mut matched = false;
    scalar_tokens(data, |token| {
        if !matched {
            window[count % 8] = solution::lookup(terms, table, token).unwrap_or(usize::MAX);
            count += 1;
            if count >= ids.len() {
                let first = count - ids.len();
                matched = (0..ids.len()).all(|j| window[(first + j) % 8] == ids[j]);
            }
        }
    });
    matched
}

fn main() {
    let root = std::env::args().nth(1).expect("sample data root");
    let qdata = fs::read(format!("{root}/queries.txt")).unwrap();
    let (terms, queries, query_ids, _) = solution::parse_queries(&qdata);
    let table = solution::ScanTable::new(&terms);
    let docs = fs::read(format!("{root}/docs.txt")).unwrap();
    let mut lines = Vec::new();
    let mut postings = vec![Vec::<u32>::new(); terms.len()];
    let mut last = vec![u32::MAX; terms.len()];
    for line in docs.split(|&b| b == b'\n').filter(|x| !x.is_empty()) {
        let tab = line.iter().position(|&b| b == b'\t').unwrap();
        let id = line[..tab].iter().fold(0u32, |n, &b| n * 10 + (b - b'0') as u32);
        lines.push(&line[tab + 1..]);
        scalar_tokens(&line[tab + 1..], |token| {
            if let Some(t) = solution::lookup(&terms, &table, token) {
                if last[t] != id { last[t] = id; postings[t].push(id); }
            }
        });
    }
    let mut checked = 0usize;
    for query in queries {
        let solution::Query::Phrase(range) = query else { continue };
        let ids = &query_ids[range];
        let anchor = *ids.iter().min_by_key(|&&t| postings[t].len()).unwrap();
        for &doc in &postings[anchor] {
            if ids.iter().any(|&t| postings[t].binary_search(&doc).is_err()) { continue; }
            let body = lines[doc as usize];
            let old = old_match(body, &ids, &terms, &table);
            let fast = solution::phrase_in_doc(body, &ids, anchor, &terms);
            assert_eq!(old, fast, "phrase verifier mismatch: doc {doc}, ids {ids:?}");
            checked += 1;
        }
    }
    println!("phrase verifier differential passed: {checked} candidate checks");
}
