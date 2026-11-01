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
    // PROT_READ = 1, MAP_PRIVATE = 2. Read-only private mapping.
    let ptr = unsafe { mmap(std::ptr::null_mut(), len, 1, 2, f.as_raw_fd(), 0) };
    if ptr == !0 as *mut std::ffi::c_void {
        return Err(std::io::Error::last_os_error());
    }
    // MADV_WILLNEED = 3: trigger asynchronous kernel readahead across entire file
    unsafe {
        madvise(ptr, len, 3);
    }
    Ok((ptr as *mut u8, len))
}
#[cfg(not(unix))]
fn mmap_file(_: &File) -> std::io::Result<(*mut u8, usize)> {
    Err(std::io::Error::new(std::io::ErrorKind::Unsupported, "unix"))
}

const INTERN_SIZE: usize = 262144;
const WORDS_PER_COUNTRY: usize = INTERN_SIZE / 64; // 4096 words per country

// FNV-1a 64-bit: raw delimited text is a bijection with unescaped field text
#[inline]
fn hash_name(b: &[u8]) -> u64 {
    let mut h = 0xcbf29ce484222325u64;
    for &x in b {
        h = (h ^ (x as u64)).wrapping_mul(0x100000001b3);
    }
    h
}

#[derive(Clone, Copy, Default)]
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
    const EMPTY: Self = Self {
        count: 0,
        id_sum: 0,
        amount_sum: 0,
        amount_min: i64::MAX,
        amount_max: i64::MIN,
        name_bytes: 0,
        note_bytes: 0,
    };
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

// Scans quoted or unquoted field; unescaped length is raw content minus escaped quotes / 2.
// Safety: chunk boundaries ensure offset stays strictly within mapped CSV records.
#[inline]
fn scan_field(bytes: &[u8], offset: &mut usize, hash: bool) -> (usize, u64) {
    if unsafe { *bytes.get_unchecked(*offset) } == b'"' {
        *offset += 1;
        let (start, mut quotes) = (*offset, 0);
        loop {
            let b = unsafe { *bytes.get_unchecked(*offset) };
            if b == b'"' {
                if unsafe { *bytes.get_unchecked(*offset + 1) } == b'"' {
                    quotes += 2;
                    *offset += 2;
                } else {
                    let raw = &bytes[start..*offset];
                    let h = if hash { hash_name(raw) } else { 0 };
                    *offset += 2; // skip quote and comma
                    return (raw.len() - quotes / 2, h);
                }
            } else {
                *offset += 1;
            }
        }
    } else {
        let start = *offset;
        while unsafe { *bytes.get_unchecked(*offset) } != b',' {
            *offset += 1;
        }
        let raw = &bytes[start..*offset];
        let h = if hash { hash_name(raw) } else { 0 };
        *offset += 1;
        (raw.len(), h)
    }
}

#[inline]
fn parse_u64(bytes: &[u8], offset: &mut usize) -> u64 {
    let mut v = 0u64;
    loop {
        let b = unsafe { *bytes.get_unchecked(*offset) };
        if b == b',' {
            *offset += 1;
            return v;
        }
        v = v * 10 + (b - b'0') as u64;
        *offset += 1;
    }
}

#[inline]
fn parse_amount(bytes: &[u8], offset: &mut usize) -> i64 {
    let neg = if unsafe { *bytes.get_unchecked(*offset) } == b'-' {
        *offset += 1;
        true
    } else {
        false
    };
    let mut d = 0i64;
    loop {
        let b = unsafe { *bytes.get_unchecked(*offset) };
        if b == b'.' {
            *offset += 1;
            break;
        }
        d = d * 10 + (b - b'0') as i64;
        *offset += 1;
    }
    let d1 = (unsafe { *bytes.get_unchecked(*offset) } - b'0') as i64;
    let d2 = (unsafe { *bytes.get_unchecked(*offset + 1) } - b'0') as i64;
    *offset += 2;
    let c = d * 100 + d1 * 10 + d2;
    if neg {
        -c
    } else {
        c
    }
}

// Interns 64-bit name hash into shared lock-free table and marks bit in country bit matrix
#[inline]
fn mark_name(intern: &[AtomicU64], matrix: &[AtomicU64], c_idx: usize, hash: u64) {
    let hash_val = if hash == 0 { 1 } else { hash };
    let mut slot = (hash_val.wrapping_mul(0x517cc1b727220a95) as usize) & (INTERN_SIZE - 1);
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
        slot = (slot + 1) & (INTERN_SIZE - 1);
    };
    let word_idx = c_idx * WORDS_PER_COUNTRY + (name_id >> 6);
    let bit_mask = 1u64 << (name_id & 63);
    let word = &matrix[word_idx];
    if (word.load(Ordering::Relaxed) & bit_mask) == 0 {
        word.fetch_or(bit_mask, Ordering::Relaxed);
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
        let rid = parse_u64(&reg_data, &mut r_idx) as usize;
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
        r_idx += 3;
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

    // Shared intern table: maps 64-bit name hash to a unique dense slot index in 0..INTERN_SIZE-1
    let intern_table: Box<[AtomicU64]> = (0..INTERN_SIZE).map(|_| AtomicU64::new(0)).collect();

    // Shared bit matrix: (num_countries + 1) rows, each WORDS_PER_COUNTRY u64 words
    let bit_matrix: Box<[AtomicU64]> = (0..(num_countries + 1) * WORDS_PER_COUNTRY)
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
                let mut local_aggs = vec![CountryAgg::EMPTY; num_countries + 1];
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
                        let (rec_id, (nb, nh), (tb, _), rid, cents) = (
                            parse_u64(bytes, &mut offset),
                            scan_field(bytes, &mut offset, true),
                            scan_field(bytes, &mut offset, false),
                            parse_u64(bytes, &mut offset) as usize,
                            parse_amount(bytes, &mut offset),
                        );
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
                        mark_name(intern_ref, matrix_ref, c_idx, nh);
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
    let mut final_aggs = vec![CountryAgg::EMPTY; num_countries + 1];
    for local in &thread_aggs {
        for (f, l) in final_aggs.iter_mut().zip(local) {
            if l.count > 0 {
                f.count += l.count;
                f.id_sum += l.id_sum;
                f.amount_sum += l.amount_sum;
                f.amount_min = f.amount_min.min(l.amount_min);
                f.amount_max = f.amount_max.max(l.amount_max);
                f.name_bytes += l.name_bytes;
                f.note_bytes += l.note_bytes;
            }
        }
    }

    // 5. Emit sorted aggregates
    let mut sorted_indices: Vec<usize> = (0..num_countries).collect();
    sorted_indices.sort_unstable_by_key(|&i| countries[i]);

    let mut emit_order: Vec<(&str, usize)> = sorted_indices
        .into_iter()
        .map(|i| (std::str::from_utf8(&countries[i]).unwrap(), i))
        .collect();
    emit_order.push(("_UNMATCHED", unmatched_idx as usize));

    let mut out = BufWriter::new(File::create(format!("{}/aggregates.txt", out_dir))?);
    for (name, idx) in emit_order {
        let a = &final_aggs[idx];
        if a.count == 0 {
            continue;
        }
        let row_start = idx * WORDS_PER_COUNTRY;
        let names_count: usize = (0..WORDS_PER_COUNTRY)
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
