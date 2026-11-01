/* Per-file CALLPIPE over a list of host paths.
 *
 * Input records are file names (one path per record), for example the
 * paths.txt file written by examples/gen-callpipe-files. For each name
 * a nested CALLPIPE reads that file with `<`, uppercases each record
 * with XLATE, and loans the result to this stage's output:
 *
 *   callpipe < file | xlate 1-* upper | *:
 *
 * There is no leading `*:`. The path is already in PATH, so the nested
 * pipeline starts at the file. A leading `*: ` on a Regina stage would
 * drain every remaining path into one nested run.
 *
 * STREAMSTATE OUTPUT is 12 when a downstream TAKE has closed the stream,
 * so the loop can stop without opening the rest of the files. SEVER INPUT
 * then stops the path list producer. That is live on the subset interpreter
 * and on the Regina helper. The nested `< file | xlate | *:` CALLPIPE
 * streams in the parent. `PIPE_REGINA_INPROC=1` still drains every path
 * first, runs, then flushes buffered output (and stops that flush if the
 * stream is severed).
 *
 * Which interpreter actually runs this file:
 *   - Windows zip with bundled regina.dll next to pipe.exe: Regina helper.
 *   - Linux/macOS with a discoverable libregina: Regina helper.
 *   - Linux/macOS with no Regina installed: the embedded subset interpreter.
 *   - PIPE_REGINA_INPROC=1: old in-process Regina drain path.
 * There is no supported switch to force the subset interpreter.
 *
 * Live streaming CALLPIPE applies to the subset interpreter, the Regina
 * helper, and PIPCMD. Use the PIPCMD demo when you want one file at a
 * time without a REXX stage (see examples/README.md).
 */
DO FOREVER
  'STREAMSTATE OUTPUT'
  IF RC = 12 THEN DO
    'SEVER INPUT'
    LEAVE
  END
  'READTO PATH'
  IF RC = 12 THEN LEAVE
  'CALLPIPE' '< ' || PATH || ' | xlate 1-* upper | *:'
END
