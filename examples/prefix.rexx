/* Prepend ARG.1 to every record.
 *
 *   pipe "literal hello | examples/prefix.rexx >>> | console"
 *   → >>>hello
 *
 *   pipe "examples/emit.rexx hi there | examples/prefix.rexx >> | console"
 *   → >>hi / >>there
 */
PARSE ARG ARG.1
PREFIX = ARG.1
IF PREFIX = '' THEN PREFIX = '>'
DO FOREVER
  'READTO LINE'
  IF RC = 12 THEN LEAVE
  'OUTPUT' PREFIX || LINE
END
