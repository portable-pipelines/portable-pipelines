Portable Pipelines (pipe)
=========================

pipe is a portable implementation of IBM CMS Pipelines (z/VM) for
Windows, macOS, and Linux. Records flow through stages connected by |.
You pass a pipeline specification as the argument to pipe.

Run from this folder

  ./pipe "literal hello| console"

A space before | becomes part of the record (a trailing blank), so
there is no space before | in that first example.

Or add this folder to PATH and run pipe from any directory:

  pipe "literal hello| console"

Examples

See the examples folder. From this folder, a guided tour:

  sh ./examples/run-examples.sh

Or try a few by hand:

  ./pipe "literal hello | examples/upper.rexx | console"
  ./pipe "< examples/lines.txt | locate /todo/ | console"
  ./pipe "literal hello | examples/upper.py | console"

REXX stages

This archive does not include Regina. Install it separately if you want
.rexx stages to run through real Regina:

  Linux (Debian/Ubuntu):  sudo apt-get install -y regina-rexx libregina3-dev
  macOS (Homebrew):       brew install regina-rexx

pipe looks for libregina.so / libregina.dylib on the usual library path
(PIPE_REGINA, REGINA_HOME, or PATH). Without Regina, .rexx stages use the
embedded subset interpreter.

Python stages

Write a .py program that reads records from its input and writes records
to its output. Use it by naming the file in the pipeline, just as you
do with a REXX stage. Each Python stage runs as its own Python 3.10 or
later process.

This archive includes the pipelines helper folder next to pipe. pipe
makes that module available to each stage for you, so you don't set
PYTHONPATH yourself (PIPE_PYTHONPATH overrides that). You still need
Python 3.10 or later on the machine:

  Linux (Debian/Ubuntu):  sudo apt-get install -y python3
  macOS (Homebrew):       brew install python

  ./pipe "literal hello | examples/upper.py | console"

Licenses

pipe is MIT OR Apache-2.0. LICENSE-MIT and LICENSE-APACHE are in this
folder. Source: https://github.com/portable-pipelines/portable-pipelines

Trademarks

CMS Pipelines and z/VM are trademarks of IBM. This project is not
affiliated with or endorsed by IBM.
