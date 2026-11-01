use std::env;
use std::fs::File;
use std::io::{BufWriter, Write};

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
    let p = unsafe { mmap(std::ptr::null_mut(), len, 1, 2, f.as_raw_fd(), 0) };
    if p == !0 as *mut std::ffi::c_void {
        return Err(std::io::Error::last_os_error());
    }
    unsafe {
        madvise(p, len, 3);
    }
    Ok((p as *mut u8, len))
}
#[cfg(not(unix))]
fn mmap_file(_: &File) -> std::io::Result<(*mut u8, usize)> {
    Err(std::io::Error::new(std::io::ErrorKind::Unsupported, "unix"))
}
#[cfg(unix)]
fn munmap_file(p: *mut u8, l: usize) {
    extern "C" {
        fn munmap(a: *mut std::ffi::c_void, l: usize) -> i32;
    }
    if !p.is_null() && l > 0 {
        unsafe {
            munmap(p as *mut std::ffi::c_void, l);
        }
    }
}

// Open-addressing set with Fibonacci hashing for distinct names
struct HashSet64 {
    slots: Vec<u64>,
    mask: usize,
    count: usize,
}
impl HashSet64 {
    fn with_capacity(cap: usize) -> Self {
        let cap = cap.next_power_of_two().max(16);
        Self {
            slots: vec![0; cap],
            mask: cap - 1,
            count: 0,
        }
    }
    #[inline]
    fn insert(&mut self, val: u64) {
        let val = if val == 0 { 0xdeadbeef_cafebabe } else { val };
        let mut idx = (val.wrapping_mul(0x517cc1b727220a95) as usize) & self.mask;
        loop {
            let occ = unsafe { *self.slots.get_unchecked(idx) };
            if occ == val {
                return;
            }
            if occ == 0 {
                unsafe {
                    *self.slots.get_unchecked_mut(idx) = val;
                }
                self.count += 1;
                if self.count * 10 >= (self.mask + 1) * 7 {
                    let mut s = HashSet64::with_capacity(self.slots.len() * 2);
                    for &v in &self.slots {
                        if v != 0 {
                            s.insert(v);
                        }
                    }
                    *self = s;
                }
                return;
            }
            idx = (idx + 1) & self.mask;
        }
    }
    fn merge(&mut self, o: &HashSet64) {
        if o.count == 0 {
            return;
        }
        while (self.count + o.count) * 10 >= (self.mask + 1) * 7 {
            let mut s = HashSet64::with_capacity(self.slots.len() * 2);
            for &v in &self.slots {
                if v != 0 {
                    s.insert(v);
                }
            }
            *self = s;
        }
        for &v in &o.slots {
            if v != 0 {
                self.insert(v);
            }
        }
    }
}

// FNV-1a 64: raw delimited text is a bijection with unescaped text
#[inline]
fn hash_name(b: &[u8]) -> u64 {
    let mut h = 0xcbf29ce484222325u64;
    for &x in b {
        h = (h ^ (x as u64)).wrapping_mul(0x100000001b3);
    }
    h
}

struct CountryAgg {
    count: u64,
    id_sum: u64,
    amount_sum: i64,
    amount_min: i64,
    amount_max: i64,
    name_bytes: u64,
    note_bytes: u64,
    names: HashSet64,
}
impl CountryAgg {
    fn new() -> Self {
        Self {
            count: 0,
            id_sum: 0,
            amount_sum: 0,
            amount_min: i64::MAX,
            amount_max: i64::MIN,
            name_bytes: 0,
            note_bytes: 0,
            names: HashSet64::with_capacity(128),
        }
    }
    #[inline]
    fn add(&mut self, id: u64, cents: i64, nb: u64, tb: u64, nh: u64) {
        self.count += 1;
        self.id_sum += id;
        self.amount_sum += cents;
        self.amount_min = self.amount_min.min(cents);
        self.amount_max = self.amount_max.max(cents);
        self.name_bytes += nb;
        self.note_bytes += tb;
        self.names.insert(nh);
    }
    fn merge(&mut self, o: &CountryAgg) {
        self.count += o.count;
        self.id_sum += o.id_sum;
        self.amount_sum += o.amount_sum;
        self.amount_min = self.amount_min.min(o.amount_min);
        self.amount_max = self.amount_max.max(o.amount_max);
        self.name_bytes += o.name_bytes;
        self.note_bytes += o.note_bytes;
        self.names.merge(&o.names);
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

// In records.csv, any LF followed by optional BOM + digit is a verified true record start
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

// Parses quoted or unquoted field; unescaped length is raw content minus escaped quotes / 2
#[inline]
fn scan_field(bytes: &[u8], offset: &mut usize) -> (usize, u64) {
    if *offset < bytes.len() && bytes[*offset] == b'"' {
        *offset += 1;
        let (start, mut quotes) = (*offset, 0);
        loop {
            if bytes[*offset] == b'"' {
                if *offset + 1 < bytes.len() && bytes[*offset + 1] == b'"' {
                    quotes += 2;
                    *offset += 2;
                } else {
                    let res = (
                        (*offset - start) - quotes / 2,
                        hash_name(&bytes[start..*offset]),
                    );
                    *offset += 2;
                    return res;
                }
            } else {
                *offset += 1;
            }
        }
    } else {
        let start = *offset;
        while *offset < bytes.len() && bytes[*offset] != b',' {
            *offset += 1;
        }
        let res = (*offset - start, hash_name(&bytes[start..*offset]));
        *offset += 1;
        res
    }
}

#[inline]
fn parse_u64(bytes: &[u8], offset: &mut usize) -> u64 {
    let mut v = 0u64;
    while *offset < bytes.len() && bytes[*offset] != b',' {
        v = v * 10 + (bytes[*offset] - b'0') as u64;
        *offset += 1;
    }
    *offset += 1;
    v
}

#[inline]
fn parse_amount(bytes: &[u8], offset: &mut usize) -> i64 {
    let neg = if *offset < bytes.len() && bytes[*offset] == b'-' {
        *offset += 1;
        true
    } else {
        false
    };
    let mut d = 0i64;
    while *offset < bytes.len() && bytes[*offset] != b'.' {
        d = d * 10 + (bytes[*offset] - b'0') as i64;
        *offset += 1;
    }
    *offset += 1;
    let c = d * 100 + (bytes[*offset] - b'0') as i64 * 10 + (bytes[*offset + 1] - b'0') as i64;
    *offset += 2;
    if neg {
        -c
    } else {
        c
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    let data_dir = args.get(1).map_or("/data", |s| s.as_str());
    let out_dir = args.get(2).map_or("/out", |s| s.as_str());

    // 1. Build flat direct-lookup table from regions.csv
    let reg_data = std::fs::read(format!("{}/regions.csv", data_dir))?;
    let (mut countries, mut reg_pairs, mut max_rid) = (Vec::new(), Vec::new(), 0usize);
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
        countries.push([reg_data[r_idx], reg_data[r_idx + 1], reg_data[r_idx + 2]]);
        reg_pairs.push((rid, *countries.last().unwrap()));
        max_rid = max_rid.max(rid);
        r_idx += 3;
        while r_idx < reg_data.len() && reg_data[r_idx] != b'\n' {
            r_idx += 1;
        }
        if r_idx < reg_data.len() {
            r_idx += 1;
        }
    }

    countries.sort_unstable();
    countries.dedup();
    let num_countries = countries.len();
    let unmatched_idx = num_countries as u8;

    let mut region_to_country = vec![unmatched_idx; max_rid + 1];
    for (rid, c) in reg_pairs {
        if let Ok(pos) = countries.binary_search(&c) {
            region_to_country[rid] = pos as u8;
        }
    }

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

    // 3. Parallel chunk processing across 12 threads
    let thread_aggs = std::thread::scope(|s| {
        let mut handles = Vec::with_capacity(num_threads);
        for t in 0..num_threads {
            let r2c = &region_to_country;
            let handle = s.spawn(move || {
                let nom_start = data_start + (t * data_len) / num_threads;
                let nom_end = data_start + ((t + 1) * data_len) / num_threads;
                let mut offset = find_first_record_start(bytes, data_start, nom_start);
                let mut local_aggs: Vec<CountryAgg> =
                    (0..=num_countries).map(|_| CountryAgg::new()).collect();

                while offset < nom_end && offset < file_len {
                    if offset + 3 <= file_len && &bytes[offset..offset + 3] == b"\xef\xbb\xbf" {
                        offset += 3;
                    }
                    let (rec_id, (nb, nh), (tb, _), rid, cents) = (
                        parse_u64(bytes, &mut offset),
                        scan_field(bytes, &mut offset),
                        scan_field(bytes, &mut offset),
                        parse_u64(bytes, &mut offset) as usize,
                        parse_amount(bytes, &mut offset),
                    );
                    if offset < file_len && bytes[offset] == b'\r' {
                        offset += 1;
                    }
                    if offset < file_len && bytes[offset] == b'\n' {
                        offset += 1;
                    }
                    let c_idx = if rid < r2c.len() {
                        r2c[rid] as usize
                    } else {
                        unmatched_idx as usize
                    };
                    local_aggs[c_idx].add(rec_id, cents, nb as u64, tb as u64, nh);
                }
                local_aggs
            });
            handles.push(handle);
        }
        handles
            .into_iter()
            .map(|h| h.join().unwrap())
            .collect::<Vec<_>>()
    });

    // 4. Merge thread accumulators
    let mut final_aggs = (0..=num_countries)
        .map(|_| CountryAgg::new())
        .collect::<Vec<_>>();
    for local_aggs in &thread_aggs {
        for i in 0..=num_countries {
            if local_aggs[i].count > 0 {
                final_aggs[i].merge(&local_aggs[i]);
            }
        }
    }

    // 5. Emit sorted aggregates
    let mut out = BufWriter::new(File::create(format!("{}/aggregates.txt", out_dir))?);
    for (i, a) in final_aggs.iter().enumerate() {
        if a.count == 0 {
            continue;
        }
        let c: &[u8] = if i < num_countries {
            &countries[i]
        } else {
            b"_UNMATCHED"
        };
        write!(
            out,
            "{} {} {} ",
            std::str::from_utf8(c).unwrap(),
            a.count,
            a.id_sum
        )?;
        write_cents(&mut out, a.amount_sum)?;
        write!(out, " ")?;
        write_cents(&mut out, a.amount_min)?;
        write!(out, " ")?;
        write_cents(&mut out, a.amount_max)?;
        writeln!(out, " {} {} {}", a.names.count, a.name_bytes, a.note_bytes)?;
    }
    out.flush()?;

    #[cfg(unix)]
    munmap_file(ptr, file_len);

    Ok(())
}
