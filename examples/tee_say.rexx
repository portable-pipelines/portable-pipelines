/* Copy each record to output and also SAY it to stderr (debug).
 *
 *   pipe "examples/emit.rexx one two | examples/tee_say.rexx | console"
 *   → stdout: one / two   stderr: one / two
 *
 * SAY is a top-level statement in the subset (not inside THEN).
 */
DO FOREVER
  'READTO LINE'
  IF RC = 12 THEN LEAVE
  SAY LINE
  'OUTPUT' LINE
END
