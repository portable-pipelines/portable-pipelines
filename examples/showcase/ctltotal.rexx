/* ctltotal.rexx - verify a print report's control totals.            */
/* Input, in any order: "D" + detail line, "T" + the TOTAL line.      */
/* Detail columns (after the tag): units 39-43, amount 46-55.         */
/* EXIT 8 when the totals disagree. Needs Regina.                     */
units = 0; cents = 0; rows = 0; want = ''
DO FOREVER
  'READTO LINE'
  IF RC <> 0 THEN LEAVE
  tag = LEFT(line, 1)
  rec = SUBSTR(line, 2)
  u = STRIP(SUBSTR(rec, 39, 5))
  a = SPACE(TRANSLATE(STRIP(SUBSTR(rec, 46, 10)), ' ', ','), 0)
  IF tag = 'T' THEN DO
    want = u a
    ITERATE
  END
  rows  = rows + 1
  units = units + u
  cents = cents + a * 100
END
got = units FORMAT(cents / 100, , 2)
IF want = '' THEN DO
  'OUTPUT' 'control total: no TOTAL line found'
  EXIT 8
END
IF got = want THEN DO
  'OUTPUT' 'control total OK:' rows 'rows,' units 'units,' WORD(got, 2)
  EXIT 0
END
'OUTPUT' 'CONTROL TOTAL MISMATCH: report says' want || ', detail adds up to' got
EXIT 8
