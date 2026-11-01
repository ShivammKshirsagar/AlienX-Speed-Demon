import sys
import os

def main():
    path = "data/c/records.csv"
    if not os.path.exists(path):
        print(f"Error: {path} not found")
        sys.exit(1)

    print(f"Reading {path}...")
    with open(path, "rb") as f:
        data = f.read()

    file_len = len(data)
    print(f"File length: {file_len} bytes")

    # Check (b):
    # - no CR appears except as first byte of CRLF
    # - no non-ASCII bytes except BOMs (EF BB BF)
    cr_errors = 0
    non_ascii_errors = 0
    bom_count = 0

    i = 0
    while i < file_len:
        b = data[i]
        if b == 0x0D:
            if i + 1 >= file_len or data[i + 1] != 0x0A:
                cr_errors += 1
            i += 1
        elif b == 0x0A:
            i += 1
        elif 0x20 <= b <= 0x7E:
            i += 1
        elif b == 0xEF and i + 2 < file_len and data[i+1] == 0xBB and data[i+2] == 0xBF:
            bom_count += 1
            i += 3
        else:
            non_ascii_errors += 1
            i += 1

    print(f"(b) Results:")
    print(f"    CR not followed by LF: {cr_errors}")
    print(f"    Non-ASCII non-BOM bytes: {non_ascii_errors}")
    print(f"    Total BOMs: {bom_count}")

    # Ground truth parse:
    # Header starts at offset 0 (or 3 if BOM)
    idx = 0
    if data.startswith(b"\xef\xbb\xbf"):
        idx = 3

    # Skip header
    header_end = data.find(b"\n", idx)
    if header_end == -1:
        print("Error: header not found")
        sys.exit(1)
    record_start = header_end + 1

    gt_starts = []
    in_quotes = False
    i = record_start
    current_rec_start = record_start

    # Check BOM at start of record
    if i < file_len and data[i:i+3] == b"\xef\xbb\xbf":
        # Note: the BOM is part of the record boundary or skipped
        # The prompt says: "every LF that is followed (after an optional EF BB BF) by an ASCII digit is a true record start"
        pass

    while i < file_len:
        b = data[i]
        if b == ord('"'):
            in_quotes = not in_quotes
            i += 1
        elif b == 0x0A and not in_quotes:
            # End of record at i. Next record begins at i + 1
            next_start = i + 1
            if next_start < file_len:
                gt_starts.append(next_start)
            i += 1
        else:
            i += 1

    # First record starts at `record_start`
    all_gt_starts = [record_start] + gt_starts
    total_records = len(all_gt_starts)
    print(f"(c) Total records: {total_records}")

    # Now test heuristic (a):
    # Candidate starts: record_start (the very first after header)
    # plus for every LF:
    # if followed by digit, or by BOM + digit
    heuristic_starts = [record_start]
    false_positives = 0
    false_negatives = 0

    gt_starts_set = set(all_gt_starts)

    # Scan for candidates
    # Candidate starting after header:
    i = header_end + 1
    # Look for all LFs
    pos = data.find(b"\n", header_end)
    detected_starts = []
    while pos != -1:
        candidate = pos + 1
        if candidate < file_len:
            c = data[candidate]
            if 0x30 <= c <= 0x39: # digit
                detected_starts.append(candidate)
            elif candidate + 3 < file_len and data[candidate:candidate+3] == b"\xef\xbb\xbf" and 0x30 <= data[candidate+3] <= 0x39:
                detected_starts.append(candidate) # whether we point before or after BOM
        pos = data.find(b"\n", pos + 1)

    all_detected = [record_start] + detected_starts
    print(f"(a) Candidate detection:")
    print(f"    Ground truth count: {len(all_gt_starts)}")
    print(f"    Detected count: {len(all_detected)}")
    if all_gt_starts == all_detected:
        print("    PERFECT MATCH: every LF followed by optional BOM + digit is a true record start, and every true record start is preceded by LF!")
    else:
        diff_missing = set(all_gt_starts) - set(all_detected)
        diff_extra = set(all_detected) - set(all_gt_starts)
        print(f"    Missing starts: {len(diff_missing)}")
        print(f"    Extra starts (false positives): {len(diff_extra)}")
        if diff_missing:
            print("    Sample missing:", list(diff_missing)[:5])
        if diff_extra:
            print("    Sample extra:", list(diff_extra)[:5])

if __name__ == "__main__":
    main()
