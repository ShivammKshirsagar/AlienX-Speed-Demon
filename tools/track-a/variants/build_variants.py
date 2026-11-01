import os
from pathlib import Path
import shutil

ROOT = Path(__file__).resolve().parents[3]
BASE = ROOT / "tools" / "track-a" / "baseline-snapshot2"
OUT = Path(__file__).resolve().parent
FILES = [".dockerignore", ".speed-demon.toml", "Cargo.lock", "Cargo.toml", "Dockerfile", "PERF.md", "README.md"]
base_source = (BASE / "src" / "main.rs").read_text()


def replace_once(text, old, new, label):
    if text.count(old) != 1:
        raise RuntimeError(f"expected exactly one {label} anchor")
    return text.replace(old, new, 1)


def emit(name, source):
    dest = OUT / name
    if dest.exists():
        shutil.rmtree(dest)
    dest.mkdir()
    for item in FILES:
        origin = ROOT / "track-a" / item if item in ("PERF.md", "README.md") else BASE / item
        shutil.copy2(origin, dest / item)
    (dest / "src").mkdir()
    (dest / "src" / "main.rs").write_text(source)


v1 = replace_once(
    base_source,
    "    drop(mapping);\n    std::process::exit(0);",
    "    unsafe extern \"C\" {\n        fn _exit(status: i32) -> !;\n    }\n"
    "    // SAFETY: output workers have joined and no destructor is needed after the write.\n"
    "    unsafe { _exit(0) }",
    "v1 exit",
)
emit("v1", v1)


scanner = r'''
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn find_line_fields_avx2(line: &[u8]) -> (usize, usize, usize) {
    use std::arch::x86_64::*;
    let (mut p, mut a, mut b, mut n) = (28, 0, 0, 0);
    while p + 32 <= line.len() {
        // SAFETY: the loop condition guarantees a full readable vector.
        let x = unsafe { _mm256_loadu_si256(line.as_ptr().add(p).cast()) };
        let s = _mm256_movemask_epi8(_mm256_cmpeq_epi8(x, _mm256_set1_epi8(b' ' as i8))) as u32;
        let e = _mm256_movemask_epi8(_mm256_cmpeq_epi8(x, _mm256_set1_epi8(b'\n' as i8))) as u32;
        let stop = if e == 0 { 32 } else { e.trailing_zeros() as usize };
        let mut m = s & if stop == 32 { u32::MAX } else { (1 << stop) - 1 };
        while m != 0 && n < 2 {
            let q = p + m.trailing_zeros() as usize;
            if n == 0 { a = q } else { b = q }
            n += 1; m &= m - 1;
        }
        if e != 0 { return (p + stop, a, b); }
        p += 32;
    }
    for i in p..line.len() { if line[i] == b'\n' { return (i, a, b); } if line[i] == b' ' && n < 2 { if n == 0 { a = i } else { b = i } n += 1; } }
    (line.len(), a, b)
}
fn parse_uint_swar(bytes: &[u8]) -> u32 {
    if bytes.len() != 8 { return parse_uint(bytes); }
    // SAFETY: exactly eight token bytes are readable; converting from LE fixes byte order.
    let word = u64::from_le(unsafe { std::ptr::read_unaligned(bytes.as_ptr().cast()) });
    let mut value = 0u32;
    for shift in 0..8 { value = value * 10 + ((word >> (shift * 8)) as u8 - b'0') as u32; }
    value
}
'''
v2 = replace_once(
    base_source,
    "fn parse_range<'a>(data: &'a [u8], start: usize, end: usize) -> Local<'a> {",
    "fn parse_range<'a>(data: &'a [u8], start: usize, end: usize) -> Local<'a> {",
    "v2 parse marker",
)
find_start = v2.index("#[cfg(target_arch = \"x86_64\")]\n#[target_feature(enable = \"avx2\")]\nunsafe fn find_byte_avx2")
find_end = v2.index("fn parse_range<'a>", find_start)
v2 = v2[:find_start] + scanner + "\n" + v2[find_end:]
v2 = replace_once(
    v2,
    "    let day_offset = match (month, day) {\n        (10, 30) => 0,\n        (10, 31) => 24,\n        (11, 1) => 48,\n        (11, 2) => 72,\n        _ => unreachable!(\"timestamp outside the specified window\"),\n    };\n    day_offset + usize::from(hour) - 18",
    "    let day = usize::from(month - 10) * 31 + usize::from(day) - 30;\n    day * 24 + usize::from(hour) - 18",
    "v2 arithmetic hour",
)
v2 = replace_once(
    v2,
    "        let line_end = find_byte(&data[*pos..end], b'\\n').map_or(end, |n| *pos + n);\n        let line = &data[*pos..line_end];\n\n        // The timestamp has fixed width; only the date and hour bytes are needed.\n        let field = 28 + find_byte(&line[28..], b' ').unwrap();\n        let endpoint_start = field + 1;\n        let endpoint_end = endpoint_start + find_byte(&line[endpoint_start..], b' ').unwrap();\n        let status_start = endpoint_end + 1;\n        let latency_start = status_start + 4;",
    "        let (line_len, field, endpoint_end) = find_line_fields(&data[*pos..end]);\n        let line_end = *pos + line_len;\n        let line = &data[*pos..line_end];\n        let endpoint_start = field + 1;\n        let status_start = endpoint_end + 1;\n        let latency_start = status_start + 4;",
    "v2 parse delimiters",
)
v2 = replace_once(v2, "        let (line_len, field, endpoint_end) = find_line_fields(&data[*pos..end]);", "        // SAFETY: the pinned release target supports AVX2 and the scanner guards every load.\n        let (line_len, field, endpoint_end) = unsafe { find_line_fields_avx2(&data[*pos..end]) };", "v2 AVX scanner")
v2 = replace_once(v2, "group.latencies.push(parse_uint(&line[latency_start..]));", "group.latencies.push(parse_uint_swar(&line[latency_start..]));", "v2 SWAR")
emit("v2", v2)


table = r'''
struct EndpointTable<'a> { slots: Vec<(&'a [u8], usize)>, len: usize }
// Empty slices mark unused slots; request endpoints are always nonempty paths.
#[rustfmt::skip]
impl<'a> EndpointTable<'a> {
    fn new() -> Self { Self { slots: vec![(&[][..], 0); 16], len: 0 } }
    fn hash(key: &[u8]) -> usize {
        let limit = key.len().min(16); let (mut h, mut at) = (key.len() as u64, 0);
        while at + 8 <= limit {
            // SAFETY: the loop guarantees eight readable bytes in the first 16-byte key prefix.
            h = h.rotate_left(13) ^ unsafe { std::ptr::read_unaligned(key.as_ptr().add(at).cast::<u64>()) }; at += 8;
        }
        for &b in &key[at..limit] { h = h.rotate_left(7) ^ u64::from(b); }
        (h ^ (h >> 29)).wrapping_mul(0xbf58476d1ce4e5b9).wrapping_add(h >> 32) as usize
    }
    fn get(&self, key: &[u8]) -> Option<usize> {
        let mask = self.slots.len() - 1; let mut at = Self::hash(key) & mask;
        while !self.slots[at].0.is_empty() { let (old, id) = self.slots[at]; if old == key { return Some(id); } at = (at + 1) & mask; }
        None
    }
    fn raw(&mut self, key: &'a [u8], id: usize) {
        let mask = self.slots.len() - 1; let mut at = Self::hash(key) & mask;
        while !self.slots[at].0.is_empty() { at = (at + 1) & mask; }
        self.slots[at] = (key, id); self.len += 1;
    }
    fn insert(&mut self, key: &'a [u8], id: usize) {
        if (self.len + 1) * 10 >= self.slots.len() * 7 {
            let cap = self.slots.len() * 2; let old = std::mem::replace(&mut self.slots, vec![(&[][..], 0); cap]); self.len = 0;
            for (key, id) in old { if !key.is_empty() { self.raw(key, id); } }
        }
        self.raw(key, id);
    }
}
'''
v3 = replace_once(base_source, "struct Group {", table + "\nstruct Group {", "v3 table")
v3 = replace_once(v3, "endpoint_ids: FastMap<&'a [u8], usize>,", "endpoint_ids: EndpointTable<'a>,", "v3 local field")
v3 = replace_once(v3, "endpoint_ids: FastMap::default(),", "endpoint_ids: EndpointTable::new(),", "v3 local init")
v3 = replace_once(v3, "if let Some(&id) = self.endpoint_ids.get(bytes) {", "if let Some(id) = self.endpoint_ids.get(bytes) {", "v3 local get")
v3 = replace_once(v3, "self.endpoint_ids.insert(bytes, id);", "self.endpoint_ids.insert(bytes, id);", "v3 local insert")
v3 = replace_once(v3, "let mut global_ids: FastMap<&[u8], usize> = FastMap::default();", "let mut global_ids = EndpointTable::new();", "v3 global init")
v3 = replace_once(v3, "if let Some(&id) = global_ids.get(endpoint) {", "if let Some(id) = global_ids.get(endpoint) {", "v3 global get")
emit("v3", v3)


v4 = replace_once(base_source, "    p99: u32,\n}", "    p99: u32,\n    arena_start: usize,\n    arena_count: usize,\n}", "v4 group fields")
v4 = replace_once(v4, "            p99: 0,\n        }", "            p99: 0,\n            arena_start: 0,\n            arena_count: 0,\n        }", "v4 group init")
v4 = replace_once(v4, "    groups: Vec<FastMap<usize, Group>>,\n}", "    groups: Vec<FastMap<usize, Group>>,\n    arena: Vec<u32>,\n}", "v4 local arena")
v4 = replace_once(v4, "            groups: (0..HOURS).map(|_| FastMap::default()).collect(),\n        }", "            groups: (0..HOURS).map(|_| FastMap::default()).collect(),\n            arena: Vec::new(),\n        }", "v4 local init")
parse_start = v4.index("fn parse_into<'a>(")
parse_end = v4.index("\nstruct Mapping {", parse_start)
v4 = v4[:parse_start] + r'''fn parse_into<'a>(data: &'a [u8], pos: &mut usize, end: usize, local: &mut Local<'a>, fill: bool) {
    while *pos < end {
        let line_end = find_byte(&data[*pos..end], b'\n').map_or(end, |n| *pos + n);
        let line = &data[*pos..line_end];
        let field = 28 + find_byte(&line[28..], b' ').unwrap();
        let endpoint_start = field + 1;
        let endpoint_end = endpoint_start + find_byte(&line[endpoint_start..], b' ').unwrap();
        let status_start = endpoint_end + 1;
        let latency_start = status_start + 4;
        let endpoint = &line[endpoint_start..endpoint_end];
        let hour = hour_index(line);
        let endpoint_id = local.endpoint_id(endpoint);
        let group = local.groups[hour].entry(endpoint_id).or_insert_with(Group::new);
        if fill {
            local.arena[group.arena_start] = parse_uint(&line[latency_start..]);
            group.arena_start += 1;
        } else {
            group.arena_count += 1;
            if line[status_start] == b'5' { group.errors += 1; }
        }
        *pos = line_end + 1;
    }
}
''' + v4[parse_end:]
v4 = replace_once(
    v4,
    "    let mut local = Local::new();\n    let mut pos = start;\n    parse_into(data, &mut pos, end, &mut local);\n    assert_eq!(pos, end);\n    local",
    "    let mut local = Local::new();\n    let mut pos = start;\n    parse_into(data, &mut pos, end, &mut local, false);\n    let total = local.groups.iter().flat_map(|g| g.values()).map(|g| g.arena_count).sum();\n    local.arena.resize(total, 0);\n    let mut at = 0;\n    for group in local.groups.iter_mut().flat_map(|g| g.values_mut()) { group.arena_start = at; at += group.arena_count; }\n    pos = start;\n    parse_into(data, &mut pos, end, &mut local, true);\n    assert_eq!(pos, end);\n    local",
    "v4 two-pass arena",
)
if ".map(|group| group.latencies.len())" not in v4:
    raise RuntimeError("missing v4 weight anchor")
v4 = v4.replace(".map(|group| group.latencies.len())", ".map(|group| group.arena_count)", 1)
v4 = replace_once(v4, "let mut range_jobs: Vec<Vec<(Vec<usize>, Vec<FastMap<usize, Group>>)>>", "let mut range_jobs: Vec<Vec<(Vec<usize>, Vec<FastMap<usize, Group>>, std::sync::Arc<Vec<u32>>)>>", "v4 range jobs type")
v4 = replace_once(v4, "    for (local, remap) in locals.into_iter().zip(remaps) {\n        let mut hour_maps = local.groups.into_iter();", "    for (local, remap) in locals.into_iter().zip(remaps) {\n        let arena = std::sync::Arc::new(local.arena);\n        let mut hour_maps = local.groups.into_iter();", "v4 arena move")
v4 = replace_once(v4, "            jobs.push((remap.clone(), maps));", "            jobs.push((remap.clone(), maps, std::sync::Arc::clone(&arena)));", "v4 arena share")
v4 = replace_once(v4, "                for (remap, hour_maps) in sources {", "                for (remap, hour_maps, arena) in sources {", "v4 merge tuple")
v4 = replace_once(
    v4,
    "                            if destination[index].latencies.is_empty() {\n                                destination[index].latencies =\n                                    std::mem::take(&mut partial.latencies);\n                            } else {\n                                destination[index].latencies.append(&mut partial.latencies);\n                            }\n                            destination[index].errors += partial.errors;",
    "                            let end = partial.arena_start;\n                            let start = end - partial.arena_count;\n                            destination[index].latencies.extend_from_slice(&arena[start..end]);\n                            destination[index].errors += partial.errors;",
    "v4 merge arena slice",
)
emit("v4", v4)


v5 = replace_once(
    base_source,
    "                    group.latencies.sort_unstable();\n                    let n = group.latencies.len();\n                    group.p50 = group.latencies[(50 * n + 99) / 100 - 1];\n                    group.p95 = group.latencies[(95 * n + 99) / 100 - 1];\n                    group.p99 = group.latencies[(99 * n + 99) / 100 - 1];",
    "                    let n = group.latencies.len();\n                    if n <= 48 {\n                        group.latencies.sort_unstable();\n                        group.p50 = group.latencies[(50 * n + 99) / 100 - 1];\n                        group.p95 = group.latencies[(95 * n + 99) / 100 - 1];\n                        group.p99 = group.latencies[(99 * n + 99) / 100 - 1];\n                    } else {\n                        let r99 = (99 * n + 99) / 100 - 1;\n                        let (left99, p99, _) = group.latencies.select_nth_unstable(r99);\n                        group.p99 = *p99;\n                        let r95 = (95 * n + 99) / 100 - 1;\n                        let (left95, p95, _) = left99.select_nth_unstable(r95);\n                        group.p95 = *p95;\n                        let r50 = (50 * n + 99) / 100 - 1;\n                        group.p50 = left95.select_nth_unstable(r50).1.to_owned();\n                    }",
    "v5 selection",
)
v5 = replace_once(v5, "                    if n <= 48 {", "                    if n == 1 {\n                        group.p50 = group.latencies[0]; group.p95 = group.p50; group.p99 = group.p50;\n                    } else if n <= 48 {", "v5 singleton")
emit("v5", v5)


v6 = replace_once(base_source, "use std::thread;", "use std::sync::atomic::{AtomicUsize, Ordering};\nuse std::thread;", "v6 atomic import")
main_start = v6.index("    let mut bounds = Vec::with_capacity(workers + 1);", v6.index("fn main()"))
main_end = v6.index("\n    let mut global_ids", main_start)
v6 = v6[:main_start] + r'''    let chunk_count = (workers * 4).min(data.len().max(1));
    let mut bounds = Vec::with_capacity(chunk_count + 1);
    bounds.push(0);
    for i in 1..chunk_count {
        let mut p = data.len() * i / chunk_count;
        while p < data.len() && data[p - 1] != b'\n' { p += 1; }
        if p > *bounds.last().unwrap() && p < data.len() { bounds.push(p); }
    }
    if *bounds.last().unwrap() < data.len() { bounds.push(data.len()); }
    let next = AtomicUsize::new(0);
    let mut locals = thread::scope(|scope| {
        let joins = (0..workers).map(|_| {
            let next = &next;
            let bounds = &bounds;
            scope.spawn(move || {
                let mut local = Local::new();
                loop {
                    let chunk = next.fetch_add(1, Ordering::Relaxed);
                    if chunk + 1 >= bounds.len() { break; }
                    let (start, end) = (bounds[chunk], bounds[chunk + 1]);
                    let mut pos = start;
                    parse_into(data, &mut pos, end, &mut local);
                    assert_eq!(pos, end);
                }
                local
            })
        }).collect::<Vec<_>>();
        joins.into_iter().map(|j| j.join().expect("parser thread panicked")).collect::<Vec<_>>()
    });
''' + v6[main_end:]
if os.environ.get("KEEP_MEMORY_FAILING_V6"):
    emit("v6", v6)
elif (OUT / "v6").exists():
    shutil.rmtree(OUT / "v6")

print("created v1-v5 independently from baseline-snapshot2; v6 remains excluded for exceeding base RSS")
