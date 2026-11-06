#!/bin/sh
# Read the Cedar Cove nested documents and write the related CSV tables.
# Safe to start from any directory:
#
#   sh ./examples/marina/to-csv.sh
#
# The pipeline itself is examples/marina/to-csv.pipe. It reads
# examples/marina/vessels.jsonl and writes four files under
# examples/marina/out/. Python is not required for this direction.

set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/../.." && pwd)

mkdir -p "$ROOT/examples/marina/out"

PIPE_KIND=
PIPE_BIN=

if [ -f "$ROOT/pipe.exe" ]; then
    PIPE_KIND=exe
    PIPE_BIN=$ROOT/pipe.exe
elif [ -f "$ROOT/pipe" ]; then
    PIPE_KIND=exe
    PIPE_BIN=$ROOT/pipe
elif command -v pipe >/dev/null 2>&1; then
    PIPE_KIND=exe
    PIPE_BIN=$(command -v pipe)
elif [ -f "$ROOT/target/release/pipe.exe" ]; then
    PIPE_KIND=exe
    PIPE_BIN=$ROOT/target/release/pipe.exe
elif [ -f "$ROOT/target/release/pipe" ]; then
    PIPE_KIND=exe
    PIPE_BIN=$ROOT/target/release/pipe
elif [ -f "$ROOT/target/debug/pipe.exe" ]; then
    PIPE_KIND=exe
    PIPE_BIN=$ROOT/target/debug/pipe.exe
elif [ -f "$ROOT/target/debug/pipe" ]; then
    PIPE_KIND=exe
    PIPE_BIN=$ROOT/target/debug/pipe
elif [ -f "$ROOT/Cargo.toml" ] && command -v cargo >/dev/null 2>&1; then
    PIPE_KIND=cargo
    PIPE_BIN=cargo
else
    echo "Cannot find pipe.exe, pipe, a build under target/, or cargo." >&2
    exit 1
fi

spec=$(cat "$SCRIPT_DIR/to-csv.pipe")

(
    cd "$ROOT" || exit 1
    if [ "$PIPE_KIND" = cargo ]; then
        cargo run -q -p pipe-cli -- "$spec"
    else
        "$PIPE_BIN" "$spec"
    fi
)

echo "Wrote CSV files in $ROOT/examples/marina/out/"
