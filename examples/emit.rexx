/* Producer: write ARG.1 … ARG.4 as separate records (no READTO).
 *
 * Useful because chained `literal a|literal b` only yields `b` (LITERAL
 * replaces the stream). Prefer:
 *
 *   pipe "examples/emit.rexx a b c d | take 2 | console"
 *   → a / b
 *
 * Prefer the built-in TAKE stage; examples/take.rexx is a REXX prototype.
 */

PARSE ARG ARG.1 ARG.2 ARG.3 ARG.4
IF ARG.1 <> '' THEN 'OUTPUT' ARG.1
IF ARG.2 <> '' THEN 'OUTPUT' ARG.2
IF ARG.3 <> '' THEN 'OUTPUT' ARG.3
IF ARG.4 <> '' THEN 'OUTPUT' ARG.4
