#!/bin/sh
# Generate a temp file set for the streaming CALLPIPE demos.
#
#   sh ./examples/gen-callpipe-files.sh
#   sh ./examples/gen-callpipe-files.sh --files 50 --lines 4 --width 48
#
# Writes under ${TMPDIR:-/tmp}/pipe-callpipe-files by default (outside
# the repo). Prints OUTDIR, PATHS, and COMMANDS so you can paste them
# into pipe.

set -eu

files=50
lines=4
width=48
outdir=''

usage() {
    echo "usage: $0 [--files N] [--lines N] [--width N] [--outdir DIR]" >&2
    exit 2
}

while [ $# -gt 0 ]; do
    case $1 in
        --files)
            [ $# -ge 2 ] || usage
            files=$2
            shift 2
            ;;
        --lines)
            [ $# -ge 2 ] || usage
            lines=$2
            shift 2
            ;;
        --width)
            [ $# -ge 2 ] || usage
            width=$2
            shift 2
            ;;
        --outdir)
            [ $# -ge 2 ] || usage
            outdir=$2
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

case $files in
    ''|*[!0-9]*|0) echo "files must be a positive integer" >&2; exit 1 ;;
esac
case $lines in
    ''|*[!0-9]*|0) echo "lines must be a positive integer" >&2; exit 1 ;;
esac
case $width in
    ''|*[!0-9]*|0) echo "width must be a positive integer" >&2; exit 1 ;;
esac

if [ -z "$outdir" ]; then
    outdir="${TMPDIR:-/tmp}/pipe-callpipe-files"
fi

case $outdir in
    *'#'*)
        echo "outdir must not contain #" >&2
        exit 1
        ;;
    *'|'*)
        echo "outdir must not contain |" >&2
        exit 1
        ;;
esac

rm -rf "$outdir"
mkdir -p "$outdir/files"
outdir=$(CDPATH= cd -- "$outdir" && pwd)
filesdir=$outdir/files

n=1
while [ "$n" -le "$files" ]; do
    name=$(awk -v n="$n" 'BEGIN{printf "f%04d.txt", n}')
    path=$filesdir/$name
    awk -v f="$n" -v nlines="$lines" -v w="$width" 'BEGIN{
        xs=""
        for (k = 1; k <= w; k++) xs = xs "x"
        for (ln = 1; ln <= nlines; ln++) {
            p = sprintf("file=%04d line=%04d ", f, ln)
            extra = w - length(p)
            if (extra < 0) extra = 0
            print p substr(xs, 1, extra)
        }
    }' > "$path"
    n=$((n + 1))
done

paths=$outdir/paths.txt
cmds=$outdir/commands.txt
: > "$paths"
: > "$cmds"

n=1
while [ "$n" -le "$files" ]; do
    name=$(awk -v n="$n" 'BEGIN{printf "f%04d.txt", n}')
    full=$filesdir/$name
    printf '%s\n' "$full" >> "$paths"
    printf 'callpipe < %s | xlate 1-* upper | *:\n' "$full" >> "$cmds"
    n=$((n + 1))
done

bytes=$((files * lines * (width + 1)))
printf 'OUTDIR=%s\n' "$outdir"
printf 'PATHS=%s\n' "$paths"
printf 'COMMANDS=%s\n' "$cmds"
printf 'FILES=%s\n' "$files"
printf 'LINES=%s\n' "$lines"
printf 'WIDTH=%s\n' "$width"
printf 'BYTES_HINT=%s\n' "$bytes"
