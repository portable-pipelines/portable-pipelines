/* Test variable pool from within a REXX stage */

/* Store some test data in a stem */
DATA.0 = 3
DATA.1 = 'first'
DATA.2 = 'second'
DATA.3 = 'third'

/* Try to use STEM OUTPUT in a nested pipeline */
'CALLPIPE stem DATA. | *:'
