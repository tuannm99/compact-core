#!/usr/bin/env sh
set -eu

rows="${1:-10000}"
output="${2:-/tmp/compact-v03-bench.jsonl}"

awk -v rows="$rows" 'BEGIN {
    for (i = 0; i < rows; i++) {
        level = (i % 3 == 0 ? "INFO" : (i % 3 == 1 ? "WARN" : "ERROR"))
        printf "{\"ts\":%d,\"level\":\"%s\",\"path\":\"service/api/v1/tenant/%d/resource/%d\"}\n",
            1700000000 + i, level, i % 16, i % 64
    }
}' > "$output"
