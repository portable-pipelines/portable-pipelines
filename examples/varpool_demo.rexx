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

/* Test 2: VARFETCH */
SAY '2. VARFETCH - read a single variable'
FRUIT = 'orange'
'CALLPIPE varfetch FRUIT | console'
SAY ''

/* Test 3: VARSET */
SAY '3. VARSET - set a variable while passing records through'
'CALLPIPE literal test | varset RESULT /success/ | console'
SAY '   Variable RESULT is now:' RESULT
SAY ''

/* Test 4: VAR as source */
SAY '4. VAR as source - read multiline variable'
LINES = 'first line' || '0A'x || 'second line'
'CALLPIPE var LINES | console'
SAY ''

/* Test 5: REXXVARS */
SAY '5. REXXVARS - list some variables (first 5)'
'CALLPIPE rexxvars | take first 5 | console'
SAY ''

/* Test 6: VARLOAD */
SAY '6. VARLOAD - load variables from records'
'CALLPIPE literal /KEY1 value one/ | literal /KEY2 value two/ | varload | console'
SAY '   KEY1 =' KEY1
SAY '   KEY2 =' KEY2
SAY ''

SAY '=== Demo complete ==='
