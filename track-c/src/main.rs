use core::arch::x86_64::*;
use std::env;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

#[cfg(unix)]
fn mmap_file(f: &File) -> std::io::Result<(*mut u8, usize)> {
    extern "C" {
        fn mmap(
            a: *mut std::ffi::c_void,
            l: usize,
            p: i32,
            fl: i32,
            d: i32,
            o: i64,
        ) -> *mut std::ffi::c_void;
        fn madvise(a: *mut std::ffi::c_void, l: usize, adv: i32) -> i32;
    }
    use std::os::unix::io::AsRawFd;
    let len = f.metadata()?.len() as usize;
    if len == 0 {
        return Ok((std::ptr::null_mut(), 0));
    }
    // Safety: passing valid fd with PROT_READ (1) and MAP_PRIVATE (2)
    let ptr = unsafe { mmap(std::ptr::null_mut(), len, 1, 2, f.as_raw_fd(), 0) };
    if ptr == !0 as *mut std::ffi::c_void {
        return Err(std::io::Error::last_os_error());
    }
    // Safety: ptr and len are valid; MADV_WILLNEED = 3 triggers kernel readahead
    unsafe {
        madvise(ptr, len, 3);
    }
    Ok((ptr as *mut u8, len))
}
#[cfg(not(unix))]
fn mmap_file(_: &File) -> std::io::Result<(*mut u8, usize)> {
    Err(std::io::Error::new(std::io::ErrorKind::Unsupported, "unix"))
}

// 2^17 (131,072) slots fits ~100k distinct names; fallback handles oversized datasets
const DEFAULT_INTERN_SIZE: usize = 131072;
const FALLBACK_INTERN_SIZE: usize = 262144;
const LARGE_DATASET_THRESHOLD: usize = 300_000_000;

// 8-byte multiply-fold hash: processes 8-byte words using high-entropy multiplier
#[inline]
fn hash_name(b: &[u8]) -> u64 {
    let len = b.len();
    if len == 0 {
        return 0;
    }
    const K: u64 = 0x517cc1b727220a95;
    let mut h = (len as u64).wrapping_mul(K);
    let mut i = 0;
    while i + 8 <= len {
        // Safety: i + 8 <= len <= b.len(), valid unaligned 8-byte read
        let val = unsafe { (b.as_ptr().add(i) as *const u64).read_unaligned() };
        h = (h ^ val).wrapping_mul(K);
        i += 8;
    }
    while i < len {
        // Safety: i < len <= b.len(), valid byte read
        h = (h ^ (unsafe { *b.get_unchecked(i) } as u64)).wrapping_mul(K);
        i += 1;
    }
    h ^ (h >> 33)
}

#[derive(Clone, Copy)]
struct CountryAgg {
    count: u64,
    id_sum: u64,
    amount_sum: i64,
    amount_min: i64,
    amount_max: i64,
    name_bytes: u64,
    note_bytes: u64,
}
impl CountryAgg {
    const fn new() -> Self {
        Self {
            count: 0,
            id_sum: 0,
            amount_sum: 0,
            amount_min: i64::MAX,
            amount_max: i64::MIN,
            name_bytes: 0,
            note_bytes: 0,
        }
    }
    #[inline]
    fn add(&mut self, id: u64, cents: i64, nb: u64, tb: u64) {
        self.count += 1;
        self.id_sum += id;
        self.amount_sum += cents;
        self.amount_min = self.amount_min.min(cents);
        self.amount_max = self.amount_max.max(cents);
        self.name_bytes += nb;
        self.note_bytes += tb;
    }
    #[inline]
    fn merge(&mut self, o: &Self) {
        if o.count > 0 {
            self.count += o.count;
            self.id_sum += o.id_sum;
            self.amount_sum += o.amount_sum;
            self.amount_min = self.amount_min.min(o.amount_min);
            self.amount_max = self.amount_max.max(o.amount_max);
            self.name_bytes += o.name_bytes;
            self.note_bytes += o.note_bytes;
        }
    }
}

fn write_cents<W: Write>(out: &mut W, c: i64) -> std::io::Result<()> {
    if c < 0 {
        out.write_all(b"-")?;
    }
    write!(
        out,
        "{}.{:02}",
        c.unsigned_abs() / 100,
        c.unsigned_abs() % 100
    )
}

// In records.csv, any LF followed by optional BOM + ASCII digit is a true record start
#[inline]
fn find_first_record_start(bytes: &[u8], data_start: usize, nominal_start: usize) -> usize {
    if nominal_start <= data_start {
        return data_start;
    }
    let (len, mut cur) = (bytes.len(), nominal_start - 1);
    while cur < len {
        if bytes[cur] == b'\n' && cur + 1 < len {
            let b = bytes[cur + 1];
            if b.is_ascii_digit()
                || (cur + 4 < len
                    && &bytes[cur + 1..cur + 4] == b"\xef\xbb\xbf"
                    && bytes[cur + 4].is_ascii_digit())
            {
                return cur + 1;
            }
        }
        cur += 1;
    }
    len
}

// Fast integer parser: reads decimal digits until delimiter comma
#[inline(always)]
unsafe fn parse_u64(bytes: &[u8], offset: &mut usize) -> u64 {
    let mut v = 0u64;
    loop {
        // Safety: caller ensures offset is within mapped buffer
        let b = *bytes.get_unchecked(*offset);
        *offset += 1;
        if b == b',' {
            return v;
        }
        v = v * 10 + (b - b'0') as u64;
    }
}

// Fast amount parser: parses signed decimal amount directly into integer cents
#[inline(always)]
unsafe fn parse_amount(bytes: &[u8], offset: &mut usize) -> i64 {
    // Safety: caller ensures offset is within mapped buffer
    let neg = if *bytes.get_unchecked(*offset) == b'-' {
        *offset += 1;
        true
    } else {
        false
    };
    let mut d = 0i64;
    loop {
        // Safety: caller ensures offset is within mapped buffer
        let b = *bytes.get_unchecked(*offset);
        *offset += 1;
        if b == b'.' {
            break;
        }
        d = d * 10 + (b - b'0') as i64;
    }
    // Safety: two fractional decimal digits always follow the decimal point
    let d1 = (*bytes.get_unchecked(*offset) - b'0') as i64;
    let d2 = (*bytes.get_unchecked(*offset + 1) - b'0') as i64;
    *offset += 2;
    let cents = d * 100 + d1 * 10 + d2;
    if neg {
        -cents
    } else {
        cents
    }
}

// AVX2 accelerated field scanner with scalar boundary fallback
#[inline(always)]
unsafe fn scan_field<const HASH: bool>(bytes: &[u8], offset: &mut usize) -> (usize, u64) {
    // Safety: caller guarantees offset is within mapped CSV record bounds
    if *bytes.get_unchecked(*offset) == b'"' {
        *offset += 1;
        let start = *offset;
        let mut curr = *offset;
        let quote_vec = _mm256_set1_epi8(b'"' as i8);
        while curr + 32 <= bytes.len() {
            // Safety: curr + 32 <= bytes.len() ensures readable 32-byte chunk
            let chunk = _mm256_loadu_si256(bytes.as_ptr().add(curr) as *const __m256i);
            let mask = _mm256_movemask_epi8(_mm256_cmpeq_epi8(chunk, quote_vec)) as u32;
            if mask != 0 {
                let k = mask.trailing_zeros() as usize;
                if mask.count_ones() == 1 && k < 31 {
                    // Safety: slice within mapped bounds
                    let raw = bytes.get_unchecked(start..curr + k);
                    *offset = curr + k + 2; // skip closing quote and delimiter comma
                    return (raw.len(), if HASH { hash_name(raw) } else { 0 });
                }
                break;
            }
            curr += 32;
        }
        // Fallback for escaped quotes or chunk boundary
        let (mut idx, mut pairs) = (curr, 0);
        loop {
            // Safety: CSV syntax ensures matching closing quote
            let b = *bytes.get_unchecked(idx);
            if b == b'"' {
                if idx + 1 < bytes.len() && *bytes.get_unchecked(idx + 1) == b'"' {
                    pairs += 1;
                    idx += 2;
                } else {
                    // Safety: slice within mapped bounds
                    let raw = bytes.get_unchecked(start..idx);
                    *offset = idx + 2;
                    return (raw.len() - pairs, if HASH { hash_name(raw) } else { 0 });
                }
            } else {
                idx += 1;
            }
        }
    } else {
        let start = *offset;
        let mut curr = *offset;
        let comma_vec = _mm256_set1_epi8(b',' as i8);
        while curr + 32 <= bytes.len() {
            // Safety: curr + 32 <= bytes.len() ensures readable 32-byte chunk
            let chunk = _mm256_loadu_si256(bytes.as_ptr().add(curr) as *const __m256i);
            let mask = _mm256_movemask_epi8(_mm256_cmpeq_epi8(chunk, comma_vec)) as u32;
            if mask != 0 {
                let k = mask.trailing_zeros() as usize;
                // Safety: slice within mapped bounds
                let raw = bytes.get_unchecked(start..curr + k);
                *offset = curr + k + 1; // skip delimiter
                return (raw.len(), if HASH { hash_name(raw) } else { 0 });
            }
            curr += 32;
        }
        let mut idx = curr;
        while *bytes.get_unchecked(idx) != b',' {
            idx += 1;
        }
        // Safety: slice within mapped bounds
        let raw = bytes.get_unchecked(start..idx);
        *offset = idx + 1;
        (raw.len(), if HASH { hash_name(raw) } else { 0 })
    }
}

// Interns 64-bit name hash into shared lock-free table; slot index is dense name ID
#[inline]
fn mark_name(
    intern: &[AtomicU64],
    matrix: &[AtomicU64],
    c_idx: usize,
    hash: u64,
    words_per_country: usize,
) {
    let hash_val = if hash == 0 { 1 } else { hash };
    let mask = intern.len() - 1;
    let mut slot = (hash_val.wrapping_mul(0x517cc1b727220a95) as usize) & mask;
    let name_id = loop {
        let curr = intern[slot].load(Ordering::Relaxed);
        if curr == hash_val {
            break slot;
        }
        if curr == 0 {
            match intern[slot].compare_exchange_weak(
                0,
                hash_val,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break slot,
                Err(actual) if actual == hash_val => break slot,
                _ => {}
            }
        }
        slot = (slot + 1) & mask;
    };
    // Guard against writing out of bounds into the bit matrix
    if (name_id >> 6) < words_per_country {
        let word = &matrix[c_idx * words_per_country + (name_id >> 6)];
        let bit = 1u64 << (name_id & 63);
        if (word.load(Ordering::Relaxed) & bit) == 0 {
            word.fetch_or(bit, Ordering::Relaxed);
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    let data_dir = args.get(1).map_or("/data", |s| s.as_str());
    let out_dir = args.get(2).map_or("/out", |s| s.as_str());

    // 1. Build flat direct-lookup table from regions.csv in a single fast pass
    let reg_data = std::fs::read(format!("{}/regions.csv", data_dir))?;
    let mut country_lut = [255u8; 17576]; // 26^3 L1 table
    let mut countries = Vec::with_capacity(256);
    let mut region_to_country = vec![255u8; 1048576];
    let mut r_idx = reg_data
        .iter()
        .position(|&b| b == b'\n')
        .map_or(0, |p| p + 1);

    while r_idx < reg_data.len() {
        if reg_data[r_idx] == b'\r' || reg_data[r_idx] == b'\n' {
            r_idx += 1;
            continue;
        }
        // Safety: parse_u64 reads within reg_data bounds
        let rid = unsafe { parse_u64(&reg_data, &mut r_idx) as usize };
        let (c0, c1, c2) = (reg_data[r_idx], reg_data[r_idx + 1], reg_data[r_idx + 2]);
        let code = ((c0 - b'A') as usize * 26 + (c1 - b'A') as usize) * 26 + (c2 - b'A') as usize;
        let cid = if country_lut[code] == 255 {
            let id = countries.len() as u8;
            country_lut[code] = id;
            countries.push([c0, c1, c2]);
            id
        } else {
            country_lut[code]
        };
        if rid >= region_to_country.len() {
            region_to_country.resize((rid + 1).max(region_to_country.len() * 2), 255);
        }
        region_to_country[rid] = cid;
        while r_idx < reg_data.len() && reg_data[r_idx] != b'\n' {
            r_idx += 1;
        }
        if r_idx < reg_data.len() {
            r_idx += 1;
        }
    }

    let num_countries = countries.len();
    let unmatched_idx = num_countries as u8;

    // 2. Mmap records.csv
    let rec_file = File::open(format!("{}/records.csv", data_dir))?;
    let (ptr, file_len) = mmap_file(&rec_file)?;
    // Safety: ptr and file_len are valid from successful mmap
    let bytes = unsafe { std::slice::from_raw_parts(ptr, file_len) };

    let mut data_start = if bytes.starts_with(b"\xef\xbb\xbf") {
        3
    } else {
        0
    };
    data_start = bytes[data_start..]
        .iter()
        .position(|&b| b == b'\n')
        .map_or(file_len, |p| data_start + p + 1);

    let data_len = file_len - data_start;
    let num_threads = 12.min(data_len.max(1));
    let num_chunks = 128.min((data_len / 32768).max(1));

    let boundaries: Vec<usize> = (0..=num_chunks)
        .map(|c| {
            if c == num_chunks {
                file_len
            } else {
                find_first_record_start(bytes, data_start, data_start + (c * data_len) / num_chunks)
            }
        })
        .collect();

    let intern_size = if file_len > LARGE_DATASET_THRESHOLD {
        FALLBACK_INTERN_SIZE
    } else {
        DEFAULT_INTERN_SIZE
    };
    let words_per_country = intern_size / 64;

    // Shared intern table: 64-bit hash keys, slot index is dense name ID
    let intern_table: Box<[AtomicU64]> = (0..intern_size).map(|_| AtomicU64::new(0)).collect();

    // Shared bit matrix: (num_countries + 1) rows, each 2^17 bits (2048 words)
    let bit_matrix: Box<[AtomicU64]> = (0..(num_countries + 1) * words_per_country)
        .map(|_| AtomicU64::new(0))
        .collect();

    // 3. Dynamic work-stealing parallel chunk processing
    let chunk_cursor = AtomicUsize::new(0);
    let r2c = &region_to_country;
    let b_ref = &boundaries;
    let intern_ref = &intern_table;
    let matrix_ref = &bit_matrix;

    let thread_aggs = std::thread::scope(|s| {
        let mut handles = Vec::with_capacity(num_threads);
        for _ in 0..num_threads {
            handles.push(s.spawn(|| {
                let mut local_aggs = vec![CountryAgg::new(); num_countries + 1];
                loop {
                    let chunk_idx = chunk_cursor.fetch_add(1, Ordering::Relaxed);
                    if chunk_idx >= num_chunks {
                        break;
                    }
                    let mut offset = b_ref[chunk_idx];
                    let end = b_ref[chunk_idx + 1];

                    while offset < end {
                        if offset + 3 <= file_len && &bytes[offset..offset + 3] == b"\xef\xbb\xbf" {
                            offset += 3;
                        }
                        // Safety: offset is within mapped file and fields adhere to CSV spec
                        let (rec_id, (nb, nh), (tb, _), rid, cents) = unsafe {
                            (
                                parse_u64(bytes, &mut offset),
                                scan_field::<true>(bytes, &mut offset),
                                scan_field::<false>(bytes, &mut offset),
                                parse_u64(bytes, &mut offset) as usize,
                                parse_amount(bytes, &mut offset),
                            )
                        };
                        if offset < file_len && bytes[offset] == b'\r' {
                            offset += 1;
                        }
                        if offset < file_len && bytes[offset] == b'\n' {
                            offset += 1;
                        }
                        let c_idx = if rid < r2c.len() && r2c[rid] != 255 {
                            r2c[rid] as usize
                        } else {
                            unmatched_idx as usize
                        };
                        local_aggs[c_idx].add(rec_id, cents, nb as u64, tb as u64);
                        mark_name(intern_ref, matrix_ref, c_idx, nh, words_per_country);
                    }
                }
                local_aggs
            }));
        }
        handles
            .into_iter()
            .map(|h| h.join().unwrap())
            .collect::<Vec<_>>()
    });

    // 4. Merge thread accumulators
    let mut final_aggs = vec![CountryAgg::new(); num_countries + 1];
    for local in &thread_aggs {
        for (f, l) in final_aggs.iter_mut().zip(local) {
            f.merge(l);
        }
    }

    // 5. Emit sorted aggregates
    let mut sorted_indices: Vec<usize> = (0..num_countries).collect();
    sorted_indices.sort_unstable_by_key(|&i| countries[i]);

    let mut out = BufWriter::new(File::create(format!("{}/aggregates.txt", out_dir))?);
    for idx in sorted_indices
        .into_iter()
        .chain(std::iter::once(unmatched_idx as usize))
    {
        let a = &final_aggs[idx];
        if a.count == 0 {
            continue;
        }
        let name = if idx < num_countries {
            std::str::from_utf8(&countries[idx]).unwrap()
        } else {
            "_UNMATCHED"
        };
        let row_start = idx * words_per_country;
        let names_count: usize = (0..words_per_country)
            .map(|w| {
                bit_matrix[row_start + w]
                    .load(Ordering::Relaxed)
                    .count_ones() as usize
            })
            .sum();

        write!(out, "{name} {} {} ", a.count, a.id_sum)?;
        write_cents(&mut out, a.amount_sum)?;
        write!(out, " ")?;
        write_cents(&mut out, a.amount_min)?;
        write!(out, " ")?;
        write_cents(&mut out, a.amount_max)?;
        writeln!(out, " {names_count} {} {}", a.name_bytes, a.note_bytes)?;
    }
    out.flush()?;

    // Exit immediately to avoid deallocation and unmapping overhead
    std::process::exit(0);
}
