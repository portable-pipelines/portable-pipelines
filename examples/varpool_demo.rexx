/* Demonstration of REXX variable pool stages
 * Usage: pipe "literal dummy | examples/varpool_demo.rexx | console"
 */

SAY '=== REXX Variable Pool Demo ==='
SAY ''

/* Test 1: STEM as source (device driver) */
SAY '1. STEM as source - reading from a stem variable'
DATA.0 = 3
DATA.1 = 'apple'
DATA.2 = 'banana'
DATA.3 = 'cherry'
'CALLPIPE stem DATA. | console'
SAY ''

/* Test 2: STEM as sink - loading from a pipeline */
SAY '2. STEM as sink - loading from pipeline into stem'
'CALLPIPE literal one | literal two | stem LOADED.'
SAY '   Loaded' LOADED.0 'items:' LOADED.1 LOADED.2
SAY ''

/* Test 3: VARFETCH */
SAY '3. VARFETCH - read a single variable'
FRUIT = 'orange'
'CALLPIPE varfetch FRUIT | console'
SAY ''

/* Test 4: VARSET */
SAY '4. VARSET - set a variable while passing records through'
'CALLPIPE literal test | varset RESULT /success/ | console'
SAY '   Variable RESULT is now:' RESULT
SAY ''

/* Test 5: VAR as source */
SAY '5. VAR as source - read multiline variable'
LINES = 'first line' || '0A'x || 'second line'
'CALLPIPE var LINES | console'
SAY ''

/* Test 6: VAR as sink */
SAY '6. VAR as sink - load content into variable'
'CALLPIPE literal line_A | literal line_B | var CONTENT'
SAY '   CONTENT has' LENGTH(CONTENT) 'chars'
SAY ''

/* Test 7: REXXVARS */
SAY '7. REXXVARS - list some variables (first 3)'
'CALLPIPE rexxvars | take first 3 | console'
SAY ''

/* Test 8: VARLOAD */
SAY '8. VARLOAD - load variables from records'
'CALLPIPE < /tmp/vardata.txt | varload | hole'
SAY '   KEY1 =' KEY1
SAY '   KEY2 =' KEY2
SAY ''

SAY '=== Demo complete ==='
