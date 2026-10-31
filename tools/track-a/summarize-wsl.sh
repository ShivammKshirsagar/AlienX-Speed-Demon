#!/usr/bin/env bash
set -euo pipefail

work="$HOME/track-a-bench"

median_column() {
  awk -F '\t' -v variant="$1" -v column="$2" 'NR > 1 && $1 == variant { print $column }' "$3" \
    | sort -n | awk '{ values[NR] = $1 } END { printf "%.3f", (values[5] + values[6]) / 2 }'
}

printf 'profiled_variant\twall_s\tuser_s\tsys_s\tmajor_faults\tminor_faults\tmax_rss_kb\n'
for variant in mmap pread; do
  printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$variant" \
    "$(median_column "$variant" 3 "$work/phases.tsv")" \
    "$(median_column "$variant" 4 "$work/phases.tsv")" \
    "$(median_column "$variant" 5 "$work/phases.tsv")" \
    "$(median_column "$variant" 6 "$work/phases.tsv")" \
    "$(median_column "$variant" 7 "$work/phases.tsv")" \
    "$(median_column "$variant" 8 "$work/phases.tsv")"
done

phase_median() {
  local variant="$1" key="$2" divisor="$3"
  awk -v key="$key" '$1 == "profile" { for (i = 2; i <= NF; i++) { split($i, pair, "="); if (pair[1] == key) print pair[2] } }' \
    "$work"/profile-"$variant"-*.log | sort -n \
    | awk -v divisor="$divisor" '{ values[NR] = $1 } END { printf "%.3f", (values[5] + values[6]) / 2 / divisor }'
}

printf '\nphase_medians_ms\tmmap\tpread\n'
for phase in mmap_ns madvise_ns boundary_ns spawn_ns parse_wall_ns merge_ns scatter_ns selection_ns endpoint_sort_ns output_format_ns write_ns total_main_ns; do
  printf '%s\t%s\t%s\n' "$phase" "$(phase_median baseline "$phase" 1000000)" "$(phase_median candidate "$phase" 1000000)"
done

printf '\nphase_rss_snapshots_mib\tmmap\tpread\n'
for phase in after_map after_bounds after_parse after_merge after_selection after_output after_write; do
  for variant in baseline candidate; do
    awk -v key="$phase" '$1 == "profile" && $2 == "rss_current_bytes" { for (i = 3; i <= NF; i++) { split($i, pair, "="); if (pair[1] == key) print pair[2] / 1048576 } }' \
      "$work"/profile-"$variant"-*.log | sort -n \
      | awk '{ values[NR] = $1 } END { printf "%s", (values[5] + values[6]) / 2 }' >"$work/.rss-${variant}-${phase}"
  done
  printf '%s\t%s\t%s\n' "$phase" "$(cat "$work/.rss-baseline-$phase")" "$(cat "$work/.rss-candidate-$phase")"
done

resource_median() {
  local variant="$1" phase="$2" key="$3" scale="$4"
  awk -v phase="$phase" -v key="$key" '$1 == "profile" && $2 == "rusage" { for (i = 3; i <= NF; i++) { split($i, pair, "="); if (pair[1] == "phase" && pair[2] == phase) wanted = 1; if (pair[1] == key && wanted) value = pair[2] } if (wanted) { print value; wanted = 0 } }' \
    "$work"/profile-"$variant"-*.log | sort -n \
    | awk -v scale="$scale" '{ values[NR] = $1 } END { printf "%.3f", (values[5] + values[6]) / 2 / scale }'
}

printf '\nphase_rusage_medians\tvariant\tuser_ms\tsystem_ms\tminor_faults\tmajor_faults\tcumulative_hwm_kb\n'
for phase in mmap boundary parse merge selection endpoint_sort output_format write; do
  for variant in mmap pread; do
    printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$phase" "$variant" \
      "$(resource_median "$variant" "$phase" user_us 1000)" \
      "$(resource_median "$variant" "$phase" system_us 1000)" \
      "$(resource_median "$variant" "$phase" minor_faults 1)" \
      "$(resource_median "$variant" "$phase" major_faults 1)" \
      "$(resource_median "$variant" "$phase" cumulative_hwm_kb 1)"
  done
done
