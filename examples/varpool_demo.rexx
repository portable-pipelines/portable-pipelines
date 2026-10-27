/* Demonstration of REXX variable pool stages.
 *
 * Usage: pipe "examples/varpool_demo.rexx | console"
 */

SAY '=== REXX variable pool demo ==='
SAY ''

/* 1. STEM as source (device driver) */
SAY '1. STEM as source'
DATA.0 = 3
DATA.1 = 'apple'
DATA.2 = 'banana'
DATA.3 = 'cherry'
'CALLPIPE stem DATA. | console'
SAY ''

/* 2. STEM as sink */
SAY '2. STEM as sink'
'CALLPIPE strliteral /one/ | strliteral append /two/ | stem LOADED.'
SAY '   Loaded' LOADED.0 'items:' LOADED.1 LOADED.2
SAY ''

/* 3. VARFETCH */
SAY '3. VARFETCH'
FRUIT = 'orange'
'CALLPIPE varfetch FRUIT | console'
SAY ''

/* 4. VARSET */
SAY '4. VARSET'
'CALLPIPE literal test | varset RESULT /success/ | console'
SAY '   Variable RESULT is now:' RESULT
SAY ''

/* 5. VAR as source (embedded newline) */
SAY '5. VAR as source'
LINES = 'first line' || '0A'x || 'second line'
'CALLPIPE var LINES | console'
SAY ''

/* 6. VAR as sink */
SAY '6. VAR as sink'
'CALLPIPE literal line_A | literal line_B | var CONTENT'
SAY '   CONTENT has' LENGTH(CONTENT) 'chars'
SAY ''

/* 7. REXXVARS */
SAY '7. REXXVARS (first 3 records)'
'CALLPIPE rexxvars | take first 3 | console'
SAY ''

/* 8. VARLOAD: create the name/value file, then load it */
SAY '8. VARLOAD'
call stream 'varpool_demo_data.txt', 'c', 'open write replace'
call lineout 'varpool_demo_data.txt', 'KEY1 value_one'
call lineout 'varpool_demo_data.txt', 'KEY2 value_two'
call stream 'varpool_demo_data.txt', 'c', 'close'
'CALLPIPE < varpool_demo_data.txt | varload | hole'
SAY '   KEY1 =' KEY1
SAY '   KEY2 =' KEY2
SAY ''

SAY '=== Demo complete ==='
