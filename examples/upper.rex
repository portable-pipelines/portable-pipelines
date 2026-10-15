/* Uppercase each input record (CMS Pipelines REXX stage example) */
DO FOREVER
  READTO LINE
  IF RC = 12 THEN LEAVE
  WRITETO TRANSLATE(LINE)
END
