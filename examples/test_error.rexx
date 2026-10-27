/* Test to see errors */
SAY 'Testing VARSET (should fail with clear error if varpool doesnt work)...'
'CALLPIPE literal test | varset TESTVAR /value/'
SAY 'VARSET completed, RC=' RC

SAY 'TESTVAR =' TESTVAR
