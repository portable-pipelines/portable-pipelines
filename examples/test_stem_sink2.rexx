/* Careful STEM sink test */
SAY 'Testing STEM as sink...'
'CALLPIPE literal one | literal two | literal three | stem TEST.'
SAY 'Pipeline completed, RC=' RC
SAY 'TEST.0 =' TEST.0
IF DATATYPE(TEST.0, 'W') THEN DO
  DO I = 1 TO TEST.0
    SAY '  TEST.'I '=' TEST.I
  END
END
