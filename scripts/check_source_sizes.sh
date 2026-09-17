#!/usr/bin/env bash
set -euo pipefail

strict=0
if [[ "${1:-}" == "--strict" ]]; then
    strict=1
fi

repository_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
failure=0

while IFS= read -r -d '' source_file; do
    line_count="$(wc -l < "${source_file}")"
    if (( line_count > 1000 )); then
        printf 'oversized source (%s lines): %s\n' "${line_count}" "${source_file#"${repository_root}/"}"
        if (( strict )); then
            failure=1
        fi
    elif (( line_count > 500 )); then
        printf 'large source (%s lines): %s\n' "${line_count}" "${source_file#"${repository_root}/"}"
    fi
done < <(
    find "${repository_root}" \
        -path "${repository_root}/.git" -prune -o \
        -path "${repository_root}/target" -prune -o \
        \( -name '*.rs' -o -name '*.wgsl' \) -type f -print0
)

exit "${failure}"
