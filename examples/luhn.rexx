/* Original Luhn mod-10 check for a digit string.
 *
 * Spaces and dashes are stripped. Other characters fail VERIFY and produce
 * no output. Writes 0 when the number is valid, 1 when it is not.
 *
 * The nested pipeline is CALLPIPE-style: glued labels (o:fanout, v:verify),
 * CHANGE, XLATE, REVERSE, DEBLOCK n, APPEND, and SPEC with IF/THEN/ELSE,
 * SET, PRINT, PICTURE, and NOWRITE.
 *
 * READTO already consumed the record, so this CALLPIPE injects it with
 * LITERAL. A leading *: / *.input: would see only unread input. PEEKTO is
 * not implemented yet. DEBLOCK FIXED concatenates its input as a byte
 * stream, so the space that flushes the checksum is APPENDed with
 * STRLITERAL / / after DEBLOCK, not sent through it. CALLPIPE is still
 * buffered: the nested
 * pipeline reads all of its input, runs to completion, then writes its
 * output. That is enough here because each card number is one record.
 */

DO FOREVER
  'READTO CARD'
  IF RC = 12 THEN LEAVE
  IF CARD = '' THEN ITERATE
  'CALLPIPE (end ?) literal ' || CARD || '| o:fanout | change / / // | change /-/ // | xlate 1-* upper | v:verify /0123456789/ | reverse | deblock 2 | append strliteral / / | spec a:1 . b:2 . if (a==" ") then if (#2==1) then print 0^=(#0//10) picture 9 1 endif set #0:=0 set #2:=0 else set (#2:=1) set (#0+=a) set (#1:=b*2) if (#1>9) then set (#1+=-9) endif set (#0+=#1) nowrite endif | *: ? o: | hole ? v: | hole'
END
