#!/bin/sh
# Run the streaming CALLPIPE file-set demos. Works from a Unix tarball, a
# Windows zip extracted on this machine, or a git clone.
#
#   sh ./examples/run-callpipe-files.sh
#   sh ./examples/run-callpipe-files.sh --demo take
#   sh ./examples/run-callpipe-files.sh --way rexx
#
# --way pipcmd (default) streams on every host. --way rexx streams only
# on the subset interpreter.

set -u

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/.." && pwd)

demo=all
way=pipcmd

usage() {
    echo "usage: $0 [--demo all|small|large|take] [--way pipcmd|rexx]" >&2
    exit 2
}

while [ $# -gt 0 ]; do
    case $1 in
        --demo)
            [ $# -ge 2 ] || usage
            demo=$2
            shift 2
            ;;
        --way)
            [ $# -ge 2 ] || usage
            way=$2
            shift 2
            ;;
        -h|--help)
            usage
            ;;
        *)
            echo "unknown option: $1" >&2
            usage
            ;;
    esac
done

case $demo in
    all|small|large|take) ;;
    *) echo "demo must be all, small, large, or take" >&2; exit 2 ;;
esac
case $way in
    pipcmd|rexx) ;;
    *) echo "way must be pipcmd or rexx" >&2; exit 2 ;;
esac

PIPE_KIND=
PIPE_BIN=
PIPE_LABEL=

if [ -f "$ROOT/pipe.exe" ]; then
    PIPE_KIND=exe
    PIPE_BIN=$ROOT/pipe.exe
    PIPE_LABEL="pipe.exe next to examples: $PIPE_BIN"
elif [ -f "$ROOT/pipe" ]; then
    PIPE_KIND=exe
    PIPE_BIN=$ROOT/pipe
    PIPE_LABEL="pipe next to examples: $PIPE_BIN"
elif command -v pipe >/dev/null 2>&1; then
    PIPE_KIND=exe
    PIPE_BIN=$(command -v pipe)
    PIPE_LABEL="pipe on PATH: $PIPE_BIN"
elif [ -f "$ROOT/target/x86_64-pc-windows-gnu/release/pipe.exe" ]; then
    PIPE_KIND=exe
    PIPE_BIN=$ROOT/target/x86_64-pc-windows-gnu/release/pipe.exe
    PIPE_LABEL="repo GNU pipe.exe: $PIPE_BIN"
elif [ -f "$ROOT/target/release/pipe.exe" ]; then
    PIPE_KIND=exe
    PIPE_BIN=$ROOT/target/release/pipe.exe
    PIPE_LABEL="repo pipe.exe: $PIPE_BIN"
elif [ -f "$ROOT/target/release/pipe" ]; then
    PIPE_KIND=exe
    PIPE_BIN=$ROOT/target/release/pipe
    PIPE_LABEL="repo pipe: $PIPE_BIN"
elif [ -f "$ROOT/Cargo.toml" ] && command -v cargo >/dev/null 2>&1; then
    PIPE_KIND=cargo
    PIPE_BIN=cargo
    PIPE_LABEL='cargo run -q -p pipe-cli --'
else
    echo "Cannot find pipe.exe, pipe, a release build under target/, or cargo. Looked next to $SCRIPT_DIR and under $ROOT." >&2
    exit 1
fi

echo "Using pipe: $PIPE_LABEL"

run_pipe() {
    spec=$1
    (
        cd "$ROOT" || exit 1
        if [ "$PIPE_KIND" = cargo ]; then
            cargo run -q -p pipe-cli -- "$spec"
        else
            "$PIPE_BIN" "$spec"
        fi
    )
}

gen() {
    files=$1
    lines=$2
    width=$3
    echo
    echo "Generating $files files, $lines lines, width $width ..."
    sh "$SCRIPT_DIR/gen-callpipe-files.sh" --files "$files" --lines "$lines" --width "$width"
}

run_demo() {
    title=$1
    spec=$2
    expect=$3
    echo
    echo "=== $title ==="
    echo "Pipeline: $spec"
    echo "Expected: $expect"
    run_pipe "$spec"
}

echo
if [ "$way" = pipcmd ]; then
    echo "Way: PIPCMD. CALLPIPE streams (live StageCtx) on Windows, Linux, and macOS."
else
    echo "Way: REXX. Streams on the subset interpreter. A Regina stage (Windows zip, or libregina on PATH) still drains, runs, then flushes."
    echo "There is no supported switch to force the subset interpreter."
fi

rexx_rel='examples/callpipe_files.rexx'

if [ "$demo" = all ] || [ "$demo" = small ] || [ "$demo" = take ]; then
    gen_out=$(gen 50 4 48)
    echo "$gen_out"
    small_paths=$(echo "$gen_out" | awk -F= '$1=="PATHS"{print substr($0, index($0,"=")+1)}')
    small_cmds=$(echo "$gen_out" | awk -F= '$1=="COMMANDS"{print substr($0, index($0,"=")+1)}')
fi

if [ "$demo" = all ] || [ "$demo" = small ]; then
    if [ "$way" = pipcmd ]; then
        spec="< $small_cmds | pipcmd | console"
    else
        spec="< $small_paths | $rexx_rel | console"
    fi
    run_demo 'a) many small files' "$spec" '200 uppercase records, FILE=0001 first, FILE=0050 last'
fi

if [ "$demo" = all ] || [ "$demo" = large ]; then
    gen_out=$(gen 3 20000 120)
    echo "$gen_out"
    large_paths=$(echo "$gen_out" | awk -F= '$1=="PATHS"{print substr($0, index($0,"=")+1)}')
    large_cmds=$(echo "$gen_out" | awk -F= '$1=="COMMANDS"{print substr($0, index($0,"=")+1)}')
    echo
    echo 'Memory: wrap with /usr/bin/time -v if you have GNU time, or watch RSS (ps, /proc). On Windows use Task Manager, Details, pipe.exe, Memory.'
    echo 'Streaming stays near a few records. Buffering would hold the whole file set (BYTES_HINT above).'
    if [ "$way" = pipcmd ]; then
        spec="< $large_cmds | pipcmd | console"
    else
        spec="< $large_paths | $rexx_rel | console"
    fi
    run_demo 'b) a few large files' "$spec" '60000 uppercase records; RSS stays far under BYTES_HINT if CALLPIPE is streaming'
fi

if [ "$demo" = all ] || [ "$demo" = take ]; then
    if [ "$way" = pipcmd ]; then
        spec="< $small_cmds | pipcmd stop anyeof | take 10 | console"
        expect='exactly 10 records (files 1 and 2 in full, then FILE=0003 LINE=0001 and LINE=0002). Files 4 through 50 are not opened.'
    else
        spec="< $small_paths | $rexx_rel | take 10 | console"
        expect='exactly 10 records. On the subset interpreter the remaining files are not read. Under Regina every path is still drained first, then 10 records are flushed.'
    fi
    run_demo 'c) take 10 stops early' "$spec" "$expect"
fi

echo
echo 'Done. See examples/README.md for the paste-ready Windows and Unix commands.'
