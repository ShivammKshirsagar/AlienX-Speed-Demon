#!/usr/bin/env bash
set -euo pipefail
stage=${1:?usage: set-diag.sh N, where N is 0, 1, 2, or 3}
[[ "$stage" =~ ^[0-3]$ ]] || { echo 'stage must be 0, 1, 2, or 3' >&2; exit 2; }
repo='/mnt/c/GitHub Desktop/AlienX-Speed-Demon'
source="$repo/track-b/src/main.rs"
[[ $(grep -Ec '^const DIAG_STAGE: u8 = [0-3];$' "$source") == 1 ]] || { echo 'expected exactly one DIAG_STAGE constant' >&2; exit 1; }
sed -i -E "s/^const DIAG_STAGE: u8 = [0-3];/const DIAG_STAGE: u8 = $stage;/" "$source"
grep '^const DIAG_STAGE:' "$source"
