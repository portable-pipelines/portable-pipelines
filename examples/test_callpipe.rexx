/* Test CALLPIPE with variable pool stages */

/* Set up test data in a stem */
PHOTOS.0 = 3
PHOTOS.1 = 'photo1.jpg'
PHOTOS.2 = 'photo2.jpg'
PHOTOS.3 = 'photo3.png'

/* Test 1: Read from PHOTOS stem (stem as first stage = source) */
SAY 'Test 1: Read from PHOTOS stem'
'CALLPIPE stem PHOTOS. | console'

/* Test 2: Load data into stem (stem as not-first = sink) */
SAY ''
SAY 'Test 2: Load into RESULT stem'
'CALLPIPE literal one | literal two | literal three | stem RESULT.'
SAY 'RESULT.0 =' RESULT.0
DO I = 1 TO RESULT.0
  SAY 'RESULT.'I '=' RESULT.I
END

/* Test 3: VAR as source */
SAY ''
SAY 'Test 3: VAR as source'
MYVAR = 'line one' || '0A'x || 'line two'
'CALLPIPE var MYVAR | console'

/* Test 4: VAR as sink */
SAY ''
SAY 'Test 4: VAR as sink'
'CALLPIPE literal alpha | literal beta | var OUTVAR.'
SAY 'OUTVAR =' OUTVAR

/* Test 5: VARFETCH */
SAY ''
SAY 'Test 5: VARFETCH'
TESTVAL = 'hello from varfetch'
'CALLPIPE varfetch TESTVAL | console'

/* Test 6: VARSET */
SAY ''
SAY 'Test 6: VARSET'
'CALLPIPE literal dummy | varset SETVAR /test value/ | console'
SAY 'SETVAR =' SETVAR

/* Test 7: REXXVARS */
SAY ''
SAY 'Test 7: REXXVARS (showing some variables)'
'CALLPIPE rexxvars | take first 5 | console'

/* Test 8: VARLOAD */
SAY ''
SAY 'Test 8: VARLOAD'
'CALLPIPE literal /LOAD1 value1/ | literal /LOAD2 value2/ | varload | console'
SAY 'LOAD1 =' LOAD1
SAY 'LOAD2 =' LOAD2
