/* Emit LENGTH and a 1-based SUBSTR window.
 *
 *   ARG.1 = start (default 1), ARG.2 = length (default remainder)
 *
 *   pipe "literal abcdef | examples/chop.rexx 2 3 | console"
 *   → 6:bcd
 *
 * STRIPs surrounding whitespace before measuring.
 */
PARSE ARG ARG.1 ARG.2
START = ARG.1
IF START = '' THEN START = 1
LEN = ARG.2
DO FOREVER
  'READTO LINE'
  IF RC = 12 THEN LEAVE
  LINE = STRIP(LINE)
  N = LENGTH(LINE)
  PIECE = SUBSTR(LINE, START, LEN)
  IF LEN = '' THEN PIECE = SUBSTR(LINE, START)
  'OUTPUT' N || ':' || PIECE
END
