/* STEM sink test with output */
SAY 'Testing STEM as filter (with console output)...'
'CALLPIPE (ENDCHAR ?) literal one ? literal two ? literal three | stem TEST. | console'
SAY 'Pipeline completed, RC=' RC  
SAY 'TEST.0 =' TEST.0
IF DATATYPE(TEST.0, 'W') THEN DO
  DO I = 1 TO TEST.0
    SAY '  TEST.'I '=' TEST.I
  END
END
