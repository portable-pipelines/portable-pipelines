#!/bin/sh
# Guided tour of Portable Pipelines. Works from a Unix tarball, a Windows zip
# extracted on this machine, or a git clone. Safe to start from any directory:
#
#   sh ./examples/run-examples.sh

set -u

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/.." && pwd)

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

python_ok=0

try_python() {
    exe=$1
    shift
    if [ -z "$exe" ]; then
        return 1
    fi
    if "$exe" "$@" -c 'import sys; raise SystemExit(0 if sys.version_info >= (3, 10) else 1)' >/dev/null 2>&1; then
        python_ok=1
        return 0
    fi
    return 1
}

if [ -n "${PIPE_PYTHON-}" ]; then
    if [ -d "$PIPE_PYTHON" ]; then
        for name in python3 python python.exe python3.exe; do
            if [ -f "$PIPE_PYTHON/$name" ]; then
                if try_python "$PIPE_PYTHON/$name"; then
                    break
                fi
            fi
        done
    else
        try_python "$PIPE_PYTHON" || true
    fi
fi

if [ "$python_ok" -eq 0 ]; then
    if command -v python3 >/dev/null 2>&1; then
        try_python "$(command -v python3)" || true
    fi
fi

if [ "$python_ok" -eq 0 ]; then
    if command -v python >/dev/null 2>&1; then
        try_python "$(command -v python)" || true
    fi
fi

if [ "$python_ok" -eq 0 ]; then
    echo "Python 3.10 or later was not found. Python demos will be skipped (install Python, or set PIPE_PYTHON)."
fi

passed=0
skipped=0
failed=0

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

show_example() {
    spec=$1
    expected=$2
    need_python=$3

    echo
    echo "Pipeline: $spec"
    echo "Expected: $expected"

    if [ "$need_python" = yes ] && [ "$python_ok" -eq 0 ]; then
        echo "Skipped: needs Python 3.10 or later on PATH (or PIPE_PYTHON)."
        skipped=$((skipped + 1))
        return 0
    fi

    code=0
    run_pipe "$spec" || code=$?
    if [ "$code" -ne 0 ]; then
        echo "(exit $code)"
        failed=$((failed + 1))
    else
        passed=$((passed + 1))
    fi
}

echo
echo "Built-in stages"
echo "---------------"

show_example \
    'literal hello| console' \
    'hello' \
    no

show_example \
    '< examples/lines.txt | locate /todo/ | console' \
    'todo: one, then todo: two' \
    no

show_example \
    'examples/emit.rexx ax b x | nlocate /x/ | console' \
    'b (records that do not contain x)' \
    no

show_example \
    '(ENDCHAR ?) examples/emit.rexx a b c d | T: take 2 | console ? T: | hole' \
    'a, then b (remainder goes to hole)' \
    no

show_example \
    'examples/emit.rexx a b c d | drop 2 | console' \
    'c, then d' \
    no

show_example \
    'literal   aa   | strip | console' \
    'aa (leading and trailing blanks removed)' \
    no

show_example \
    'literal xx| change /xx/ /yy/ | console' \
    'yy' \
    no

show_example \
    'literal c|literal a|literal b| sort | console' \
    'a, then b, then c' \
    no

show_example \
    'examples/emit.rexx a b c | count lines | console' \
    '3' \
    no

echo
echo "REXX stage"
echo "----------"

show_example \
    'literal hello| examples/upper.rexx | console' \
    'HELLO' \
    no

echo
echo "Python stages"
echo "-------------"

show_example \
    'literal hello| examples/upper.py | console' \
    'HELLO' \
    yes

show_example \
    'examples/emit.py apple banana apricot | examples/upper.py | console' \
    'APPLE, BANANA, APRICOT' \
    yes

show_example \
    'disk examples/lines.txt | examples/wordfreq.py 5 | console' \
    '2 todo, then one each of apple, banana, apricot, spaced' \
    yes

echo
echo "REXX and Python together"
echo "------------------------"

show_example \
    'examples/emit.rexx a b c | examples/upper.py | console' \
    'A, then B, then C' \
    yes

echo
echo "Multistream"
echo "-----------"

show_example \
    '(ENDCHAR ?) examples/emit.rexx apple banana apricot | L: locate /ap/ | console ? L: | examples/prefix.rexx miss= | console' \
    'apple, apricot, and miss=banana on the other stream' \
    no

show_example \
    '(end ?) < examples/users.txt | a: strfind /USER / | spec 1-* 1 / / next | j: juxtapose | console ? a: | j:' \
    'USER AAA MDISK 191, USER AAA MDISK 192, USER AAA MDISK 193, USER BBB MDISK 191, USER BBB MDISK 192' \
    no

echo
echo "Tour finished: $passed ran, $skipped skipped (Python), $failed failed."
if [ "$failed" -ne 0 ]; then
    exit 1
fi
