/* Test if varpool works at all when not in a nested pipeline */

/* Set a variable */
TEST.0 = 3
TEST.1 = 'first'
TEST.2 = 'second'
TEST.3 = 'third'

SAY 'TEST.0 =' TEST.0
SAY 'Starting pipeline...'

/* Try to read from it in a nested pipeline */
'CALLPIPE stem TEST. | count lines | var COUNT.'

SAY 'COUNT =' COUNT
