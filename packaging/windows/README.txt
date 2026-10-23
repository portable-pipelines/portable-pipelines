Portable Pipelines (pipe)
=========================

pipe is a portable implementation of IBM CMS Pipelines (z/VM) for Windows.
Records flow through stages connected by |. You pass a pipeline
specification as the argument to pipe.exe.

Run from this folder

  pipe.exe "literal hello | console"

Or add this folder to PATH and run pipe from any directory:

  pipe "literal hello | console"

Examples

See the examples folder. From this folder:

  pipe.exe "literal hello | examples\upper.rexx | console"
  pipe.exe "< examples\lines.txt | locate /TODO/ | console"

REXX stages

This zip includes Regina REXX (regina.dll and en.mtb) so .rexx stages
run through real Regina. pipe.exe looks for regina.dll in the same
folder as itself. If you remove those files, .rexx stages use the
embedded subset interpreter unless Regina is installed elsewhere
(PIPE_REGINA, REGINA_HOME, or PATH).

Python stages

Write a .py program that reads records from its input and writes records
to its output. Use it by naming the file in the pipeline, just as you
do with a REXX stage. Each Python stage runs as its own Python 3.10 or
later process, and REXX and Python stages can sit side by side in one
pipeline.

This zip includes the pipelines helper folder next to pipe.exe. pipe
makes the pipelines helper module available to each stage for you, so
you don't set PYTHONPATH yourself (PIPE_PYTHONPATH overrides that). You
still need Python 3.10 or later on the machine (PIPE_PYTHON, the py -3
launcher, python3, or python on PATH).

  pipe.exe "literal hello | examples\upper.py | console"
  pipe.exe "examples\emit.py a b c | examples\upper.py | console"

Licenses

pipe is MIT OR Apache-2.0. LICENSE-MIT and LICENSE-APACHE are in this
folder. Source: https://github.com/portable-pipelines/portable-pipelines

Regina REXX is redistributed under LICENSE-Regina.txt in this folder.
The MinGW runtime DLLs, if present, are covered by the GCC runtime
exception; they are required only when pipe.exe is linked against them.

Trademarks

CMS Pipelines and z/VM are trademarks of IBM. This project is not
affiliated with or endorsed by IBM.
