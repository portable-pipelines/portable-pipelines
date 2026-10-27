/* Photo workflow: load names into a stem, LOOKUP leftovers, act on them.
 *
 * Creates two sample lists in the current directory, loads every photo name
 * into PHOTOS., looks up against an already-processed list in DONE., and
 * puts unmatched names into TODO. Then prints the command you would run
 * for each leftover (exiftool here; copy or erase is the same pattern).
 *
 * LOOKUP secondary output is unmatched details, so TODO is wired there:
 *
 *   (end ?) stem PHOTOS. | l: lookup | hole ? stem DONE. | l: | stem TODO.
 *
 * Usage, from a writable directory (Windows cmd / PowerShell / Unix):
 *
 *   pipe "examples/photo_workflow.rexx | console"
 *
 * On Windows, if you are in the folder that contains pipe.exe:
 *
 *   pipe.exe "examples\photo_workflow.rexx | console"
 */

SAY 'Photo workflow'
SAY ''

/* Sample files in the current directory so this runs on Windows too. */
allfile = 'photo_workflow_all.txt'
donefile = 'photo_workflow_done.txt'
call stream allfile, 'c', 'open write replace'
call lineout allfile, 'vacation/img001.jpg'
call lineout allfile, 'vacation/img002.jpg'
call lineout allfile, 'vacation/img003.jpg'
call lineout allfile, 'beach/img010.jpg'
call lineout allfile, 'already_done.jpg'
call stream allfile, 'c', 'close'
call stream donefile, 'c', 'open write replace'
call lineout donefile, 'vacation/img001.jpg'
call lineout donefile, 'already_done.jpg'
call stream donefile, 'c', 'close'

SAY 'Wrote' allfile 'and' donefile

'CALLPIPE < ' || allfile || ' | stem PHOTOS.'
IF RC <> 0 THEN DO
  SAY 'CALLPIPE STEM PHOTOS failed RC=' RC
  EXIT RC
END
SAY 'Total photos:' PHOTOS.0

'CALLPIPE < ' || donefile || ' | stem DONE.'
IF RC <> 0 THEN DO
  SAY 'CALLPIPE STEM DONE failed RC=' RC
  EXIT RC
END
SAY 'Already processed:' DONE.0

SAY ''
SAY 'LOOKUP unmatched photo names into TODO.'
'CALLPIPE (end ?) stem PHOTOS. | l: lookup | hole ? stem DONE. | l: | stem TODO.'
IF RC <> 0 THEN DO
  SAY 'CALLPIPE LOOKUP failed RC=' RC
  EXIT RC
END
SAY 'Leftover photos:' TODO.0
DO I = 1 TO TODO.0
  SAY '  -' TODO.I
END

SAY ''
SAY 'Commands to run:'
DO I = 1 TO TODO.0
  photo = TODO.I
  SAY '  exiftool -overwrite_original -Comment=reviewed' photo
END

SAY ''
SAY 'Workflow complete'
