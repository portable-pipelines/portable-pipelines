/* Forward at most ARG.1 records, then LEAVE.
 *
 * Prefer the built-in TAKE stage for new pipes:
 *   pipe "examples/emit.rexx a b c d | take 2 | console"
 *
 * This REXX prototype remains for engine demos:
 *   pipe "examples/emit.rexx a b c d | examples/take.rexx 2 | console"
 *   → a / b
 *
 * Default is 1 when ARG.1 is omitted. (Counter uses a tally string so we
 * stay within the subset — no arithmetic operators.)
 */
PARSE ARG ARG.1
LIMIT = ARG.1
IF LIMIT = '' THEN LIMIT = 1
TALLY = ''
DO FOREVER
  'READTO LINE'
  IF RC = 12 THEN LEAVE
  IF LENGTH(TALLY) = LIMIT THEN LEAVE
  'OUTPUT' LINE
  TALLY = TALLY || 'x'
END
