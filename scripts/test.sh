#!/usr/bin/env bash
# Adaptive test runner: scales parallelism to available CPU and RAM.
# Usage: ./test.sh [extra cargo test args...]
#   e.g: ./test.sh --workspace
#        ./test.sh -p cogent-cli

set -euo pipefail

# --- Resource detection ---
NCPU=$(nproc 2>/dev/null || sysctl -n hw.logicalcpu 2>/dev/null || echo 4)
FREE_MB=$(awk '/MemAvailable/{print int($2/1024)}' /proc/meminfo 2>/dev/null \
          || vm_stat 2>/dev/null | awk '/free/{print int($3*4096/1048576)}' \
          || echo 2048)

# Heuristic: each rustc process needs ~300 MB, each test thread ~50 MB.
# Allow at most NCPU build jobs but cap if RAM is tight.
MAX_BUILD_JOBS=$(( FREE_MB / 300 ))
MAX_BUILD_JOBS=$(( MAX_BUILD_JOBS < 1 ? 1 : MAX_BUILD_JOBS ))
MAX_BUILD_JOBS=$(( MAX_BUILD_JOBS > NCPU ? NCPU : MAX_BUILD_JOBS ))

# Test threads: lighter, but still cap if very low memory.
MAX_TEST_THREADS=$(( FREE_MB / 100 ))
MAX_TEST_THREADS=$(( MAX_TEST_THREADS < 1 ? 1 : MAX_TEST_THREADS ))
MAX_TEST_THREADS=$(( MAX_TEST_THREADS > NCPU ? NCPU : MAX_TEST_THREADS ))

echo "System: ${NCPU} CPUs, ${FREE_MB} MB free RAM"
echo "Build jobs: ${MAX_BUILD_JOBS}, test threads: ${MAX_TEST_THREADS}"
echo ""

export CARGO_BUILD_JOBS="${MAX_BUILD_JOBS}"
export RUST_TEST_THREADS="${MAX_TEST_THREADS}"

# If --workspace is requested (or no args), run in batches to avoid OOM spike
# from compiling all crates simultaneously.
if [[ "$*" == *"--safe"* ]]; then
    echo "Safe mode: testing ONE crate at a time with CARGO_BUILD_JOBS=1..."
    export CARGO_BUILD_JOBS=1
    export RUST_TEST_THREADS=1
    ALL_CRATES=(
        cogent-common ast-parse-ts cogent-server cogent-cli
        debt-scan doc-coverage crap-metric coupling risk-map
        duplication taint-scan fuzz-surface mutation-test prop-cov
    )
    FAILED=0
    for crate in "${ALL_CRATES[@]}"; do
        echo ""
        echo "==> crate: $crate"
        if ! cargo test -p "$crate" -- --test-threads=1; then
            FAILED=1
        fi
    done
    exit $FAILED
fi

if [[ $# -eq 0 ]] || [[ "$*" == *"--workspace"* ]]; then
    echo "Running tests in batches (avoids peak memory from --workspace compilation)..."
    BATCHES=(
        "-p cogent-common"
        "-p ast-parse-ts"
        "-p cogent-server -p cogent-cli"
        "-p debt-scan -p doc-coverage"
        "-p crap-metric -p coupling"
        "-p risk-map -p duplication"
        "-p taint-scan -p fuzz-surface"
        "-p mutation-test -p prop-cov"
        "-p line-length -p halstead -p secrets -p dead-code"
        "-p cohesion -p comment-ratio -p error-handling"
        "-p type-coverage -p vuln-scan"
        "-p sast -p crypto-check -p licenses -p sbom"
    )
    FAILED=0
    for batch in "${BATCHES[@]}"; do
        echo ""
        echo "==> batch: $batch"
        # shellcheck disable=SC2086
        if ! cargo test $batch -- --test-threads="${MAX_TEST_THREADS}"; then
            FAILED=1
        fi
    done
    exit $FAILED
else
    exec cargo test "$@" -- --test-threads="${MAX_TEST_THREADS}"
fi
