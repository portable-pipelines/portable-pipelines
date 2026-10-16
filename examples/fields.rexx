/* Split each record into first word + remainder, then emit "rest|first".
 *
 *   pipe "literal red green blue | examples/fields.rexx | console"
 *   → green blue|red
 *
 * Exercises PARSE VAR (first token vs remainder).
 */
DO FOREVER
  'READTO LINE'
  IF RC = 12 THEN LEAVE
  PARSE VAR LINE REST
  'OUTPUT' REST || '|' || LINE
END
