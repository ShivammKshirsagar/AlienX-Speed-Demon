use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};
use std::time::Instant;

#[derive(Default)]
struct H(u64);
impl Hasher for H {
    fn write(&mut self, bytes: &[u8]) {
        let mut h = if self.0 == 0 { 0xcbf29ce484222325 } else { self.0 };
        let mut chunks = bytes.chunks_exact(8);
        for chunk in &mut chunks { h = h.rotate_left(5) ^ u64::from_ne_bytes(chunk.try_into().unwrap()); }
        if !chunks.remainder().is_empty() {
            let tail = chunks.remainder(); let mut word = [0; 8];
            word[..tail.len()].copy_from_slice(tail);
            h = h.rotate_left(5) ^ u64::from_ne_bytes(word);
        }
        self.0 = h ^ bytes.len() as u64;
    }
    fn finish(&self) -> u64 { self.0 }
}
type Map<K, V> = HashMap<K, V, BuildHasherDefault<H>>;

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn find_avx2(bytes: &[u8], needle: u8) -> Option<usize> {
    use std::arch::x86_64::*;
    let key = _mm256_set1_epi8(needle as i8); let mut i = 0;
    while i + 32 <= bytes.len() {
        // SAFETY: the loop condition guarantees a complete 32-byte vector is in bounds.
        let block = unsafe { _mm256_loadu_si256(bytes.as_ptr().add(i).cast()) };
        let mask = _mm256_movemask_epi8(_mm256_cmpeq_epi8(block, key)) as u32;
        if mask != 0 { return Some(i + mask.trailing_zeros() as usize); }
        i += 32;
    }
    bytes[i..].iter().position(|&b| b == needle).map(|n| i + n)
}
#[inline]
fn find(bytes: &[u8], byte: u8) -> Option<usize> {
    #[cfg(all(target_arch = "x86_64", target_feature = "avx2"))]
    { // SAFETY: benchmark builds use target-cpu=x86-64-v3.
        return unsafe { find_avx2(bytes, byte) };
    }
    #[cfg(not(all(target_arch = "x86_64", target_feature = "avx2")))]
    bytes.iter().position(|&b| b == byte)
}
fn hour(ts: &[u8]) -> usize {
    let month = (ts[5] - b'0') * 10 + ts[6] - b'0';
    let day = (ts[8] - b'0') * 10 + ts[9] - b'0';
    let h = usize::from((ts[11] - b'0') * 10 + ts[12] - b'0');
    let first = match (month, day) { (10,30) => 0, (10,31) => 24, (11,1) => 48, (11,2) => 72, _ => unreachable!() };
    first + h - 18
}
fn parse_four(bytes: &[u8]) -> u32 {
    let digits = u32::from_le_bytes(bytes[..4].try_into().unwrap()) - 0x30303030;
    let pairs = (digits & 0x00ff00ff) * 10 + ((digits >> 8) & 0x00ff00ff);
    (pairs & 0xffff) * 100 + (pairs >> 16)
}
fn uint(bytes: &[u8]) -> u32 {
    let (mut value, offset) = if bytes.len() >= 8 {
        (parse_four(&bytes[..4]) * 10_000 + parse_four(&bytes[4..8]), 8)
    } else if bytes.len() >= 4 {
        (parse_four(bytes), 4)
    } else { (0, 0) };
    for &byte in &bytes[offset..] { value = value * 10 + u32::from(byte - b'0'); }
    value
}
#[repr(C)]
struct Timespec { sec: i64, nsec: i64 }
fn thread_ns() -> u64 {
    unsafe extern "C" { fn clock_gettime(clock: i32, ts: *mut Timespec) -> i32; }
    let mut t = Timespec { sec: 0, nsec: 0 };
    // SAFETY: clock_gettime fills this timespec for CLOCK_THREAD_CPUTIME_ID (3).
    assert_eq!(unsafe { clock_gettime(3, &mut t) }, 0);
    t.sec as u64 * 1_000_000_000 + t.nsec as u64
}
#[derive(Default)]
struct Group { count: u32, errors: u32, values: Vec<u32> }
struct ResultRow { lines: usize, cpu_ns: u64 }

fn run(data: &[u8], rung: usize, workers: usize) -> (u128, usize, u64) {
    let mut bounds = vec![0];
    for i in 1..workers {
        let mut p = data.len() * i / workers;
        while p < data.len() && data[p - 1] != b'\n' { p += 1; }
        bounds.push(p);
    }
    bounds.push(data.len());
    let start = Instant::now();
    let rows = std::thread::scope(|scope| {
        let handles: Vec<_> = bounds.windows(2).map(|b| {
            let (lo, hi) = (b[0], b[1]);
            scope.spawn(move || {
                let cpu_start = thread_ns();
                let mut lines = 0; let mut pos = lo;
                let mut ids: Map<&[u8], usize> = Map::default();
                let mut groups: Vec<Map<usize, Group>> = (0..72).map(|_| Map::default()).collect();
                while pos < hi {
                    let end = find(&data[pos..hi], b'\n').map_or(hi, |n| pos + n);
                    lines += 1;
                    if rung >= 2 {
                        let line = &data[pos..end];
                        let field = 28 + find(&line[28..], b' ').unwrap();
                        let ep0 = field + 1;
                        let ep1 = ep0 + find(&line[ep0..], b' ').unwrap();
                        let h = hour(line);
                        if rung >= 3 {
                            let next = ids.len();
                            let id = *ids.entry(&line[ep0..ep1]).or_insert(next);
                            if rung >= 4 {
                                let status = ep1 + 1;
                                let latency = uint(&line[status + 4..]);
                                let error = u32::from(line[status] == b'5');
                                if rung >= 5 {
                                    let group = groups[h].entry(id).or_default();
                                    group.count += 1; group.errors += error;
                                    if rung >= 6 { group.values.push(latency); }
                                }
                            }
                        }
                    }
                    pos = if end < hi { end + 1 } else { hi };
                }
                let _keep_alive = (ids.len(), groups.iter().map(Map::len).sum::<usize>());
                ResultRow { lines, cpu_ns: thread_ns() - cpu_start }
            })
        }).collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect::<Vec<_>>()
    });
    let wall = start.elapsed().as_nanos();
    (wall, rows.iter().map(|r| r.lines).sum(), rows.iter().map(|r| r.cpu_ns).sum())
}

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("input path");
    let data = std::fs::read(path).expect("read data before the timed section");
    for workers in [12, 1] {
        for rung in 1..=6 {
            let (wall, lines, cpu) = run(&data, rung, workers);
            println!("workers={workers} rung={rung} lines={lines} thread_cpu_ns_per_line={:.1} wall_ms={:.3}",
                cpu as f64 / lines as f64, wall as f64 / 1e6);
        }
    }
}
