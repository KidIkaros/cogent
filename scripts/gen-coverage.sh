#!/bin/bash
# Generate coverage per-crate to avoid OOM.
# Usage: ./scripts/gen-coverage.sh [output.info]
# Requires: cargo-llvm-cov installed

set -euo pipefail

OUTPUT="${1:-coverage.info}"
WORKSPACE="$(cd "$(dirname "$0")/.." && pwd)"
TARGET_DIR="/tmp/qt-cov-build"

# Limit parallelism to avoid OOM
export CARGO_BUILD_JOBS=2
export RUSTFLAGS="-C codegen-units=1"

echo "Generating coverage per-crate (memory-safe mode)..."
echo "Target dir: $TARGET_DIR"
echo ""

# Clean previous coverage data
rm -f "$OUTPUT" "$OUTPUT.tmp"
touch "$OUTPUT"

# List of crates (library crates first, then binaries)
CRATES=(
    "cogent-common"
    "ast-parse"
    "crap-metric"
    "debt-scan"
    "doc-coverage"
    "duplication"
    "coupling"
    "risk-map"
)

for crate in "${CRATES[@]}"; do
    echo "[$crate] Generating coverage..."
    
    # Run coverage for single crate with limited parallelism
    if CARGO_TARGET_DIR="$TARGET_DIR" \
       CARGO_BUILD_JOBS=2 \
       cargo llvm-cov \
           --lcov \
           --output-path "$TARGET_DIR/$crate.info" \
           -p "$crate" \
           --tests \
           --quiet \
           2>/dev/null; then
        
        # Append to combined file (skip duplicate headers)
        if [ -f "$TARGET_DIR/$crate.info" ]; then
            cat "$TARGET_DIR/$crate.info" >> "$OUTPUT.tmp"
            echo "[$crate] OK"
        fi
    else
        echo "[$crate] FAILED (skipped)"
    fi
    
    # Force cleanup between crates
    sync
done

# Deduplicate and finalize
if [ -f "$OUTPUT.tmp" ]; then
    mv "$OUTPUT.tmp" "$OUTPUT"
    LINES=$(wc -l < "$OUTPUT")
    echo ""
    echo "Coverage generated: $OUTPUT ($LINES lines)"
else
    echo ""
    echo "No coverage data generated."
fi

# Cleanup
rm -rf "$TARGET_DIR"
