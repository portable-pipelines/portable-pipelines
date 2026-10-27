/* Simple test of STEM as sink */

SAY 'Before: RESULT.0 =' RESULT.0

'CALLPIPE literal one | literal two | literal three | stem RESULT.'

SAY 'After: RESULT.0 =' RESULT.0
IF RESULT.0 = 'RESULT.0' THEN DO
  SAY 'ERROR: Variable was not set!'
END
ELSE DO
  SAY 'SUCCESS: RESULT.0 is' RESULT.0
  DO I = 1 TO RESULT.0
    SAY 'RESULT.'I '=' RESULT.I
  END
END
