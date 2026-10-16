/* Uppercase and bracket each record.
 *
 *   pipe "literal hi | examples/wrap.rexx | console"
 *   → [HI]
 *
 * Exercises TRANSLATE, || concatenation, and multi-step assignment.
 */
DO FOREVER
  READTO LINE
  IF RC = 12 THEN LEAVE
  U = TRANSLATE(LINE)
  OUT = '[' || U || ']'
  OUTPUT OUT
END
