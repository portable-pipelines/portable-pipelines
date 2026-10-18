/* Producer: write ARG.1 … ARG.n as separate records (no READTO).
 *
 * A cascade of LITERAL stages writes in reverse order (literal a|literal b
 * yields b then a). Prefer emit when the records should stay in argument
 * order:
 *
 *   pipe "examples/emit.rexx a b c d | take 2 | console"
 *   → a / b
 *
 * ARG.1–ARG.12 are written when non-empty (README examples use at most 9).
 * Prefer the built-in TAKE stage; examples/take.rexx is a REXX prototype.
 */

PARSE ARG ARG.1 ARG.2 ARG.3 ARG.4 ARG.5 ARG.6 ARG.7 ARG.8 ARG.9 ARG.10 ARG.11 ARG.12
IF ARG.1 <> '' THEN 'OUTPUT' ARG.1
IF ARG.2 <> '' THEN 'OUTPUT' ARG.2
IF ARG.3 <> '' THEN 'OUTPUT' ARG.3
IF ARG.4 <> '' THEN 'OUTPUT' ARG.4
IF ARG.5 <> '' THEN 'OUTPUT' ARG.5
IF ARG.6 <> '' THEN 'OUTPUT' ARG.6
IF ARG.7 <> '' THEN 'OUTPUT' ARG.7
IF ARG.8 <> '' THEN 'OUTPUT' ARG.8
IF ARG.9 <> '' THEN 'OUTPUT' ARG.9
IF ARG.10 <> '' THEN 'OUTPUT' ARG.10
IF ARG.11 <> '' THEN 'OUTPUT' ARG.11
IF ARG.12 <> '' THEN 'OUTPUT' ARG.12
