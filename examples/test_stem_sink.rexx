/* Test STEM as sink */
SAY 'Before CALLPIPE: RESULT.0 =' RESULT.0

'CALLPIPE literal one | literal two | stem RESULT.'

SAY 'After CALLPIPE: RESULT.0 =' RESULT.0

IF DATATYPE(RESULT.0, 'W') THEN DO
  SAY 'SUCCESS! Result has' RESULT.0 'items'
  DO I = 1 TO RESULT.0
    SAY '  RESULT.'I '=' RESULT.I
  END
END
ELSE DO
  SAY 'FAILURE: RESULT.0 was not set to a number'
END
