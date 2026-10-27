/* Example REXX program demonstrating variable pool stages.
 *
 * This shows a realistic workflow: load a file into a REXX stem,
 * process it through a pipeline that uses LOOKUP, then act on the results
 * in REXX (here we just display them, but in a real case you might
 * rename/move files, call external tools, etc.).
 *
 * Run with:
 *   regina examples/photo_workflow.rexx
 */

SAY 'Creating sample data files...'

/* Create a sample "all photos" file */
CALL LINEOUT 'photos_all.txt', 'IMG_001.jpg'
CALL LINEOUT 'photos_all.txt', 'IMG_002.jpg'
CALL LINEOUT 'photos_all.txt', 'IMG_003.jpg'
CALL LINEOUT 'photos_all.txt', 'IMG_004.jpg'
CALL LINEOUT 'photos_all.txt', 'IMG_005.jpg'
CALL STREAM 'photos_all.txt', 'C', 'CLOSE'

/* Create a sample "processed" file */
CALL LINEOUT 'photos_done.txt', 'IMG_002.jpg'
CALL LINEOUT 'photos_done.txt', 'IMG_004.jpg'
CALL STREAM 'photos_done.txt', 'C', 'CLOSE'

SAY ''
SAY 'Step 1: Load all photo names into a REXX stem using STEM INPUT'

/* Use ADDRESS PIPELINE to run a pipeline that populates photos. stem */
ADDRESS PIPELINE 'disk photos_all.txt | stem input photos.'

SAY 'Loaded' photos.0 'photos into stem:'
DO i = 1 TO photos.0
  SAY '  ' photos.i
END

SAY ''
SAY 'Step 2: Use LOOKUP to find which photos have NOT been processed'

/* Feed the stem into a pipeline with LOOKUP against the done file */
ADDRESS PIPELINE,
  'stem output photos. |',
  'lookup detail disk photos_done.txt |',
  'stem input remaining.'

SAY 'Found' remaining.0 'photos still to process:'
DO i = 1 TO remaining.0
  SAY '  ' remaining.i
END

SAY ''
SAY 'Step 3: Store result in a simple variable using VAR INPUT'

/* Convert the remaining count to a status message */
ADDRESS PIPELINE,
  'literal Remaining:' remaining.0 '|',
  'var input STATUS'

SAY 'Status stored in STATUS variable:' STATUS

SAY ''
SAY 'Step 4: Use VARSET to set a variable, then VARFETCH to read it'

/* VARSET sets a variable inline while copying records through */
ADDRESS PIPELINE,
  'literal test-record |',
  'varset MYVAR /Value set by VARSET/ |',
  'hole'

/* VARFETCH retrieves the variable */
ADDRESS PIPELINE,
  'varfetch MYVAR |',
  'var input FETCHED'

SAY 'VARFETCH retrieved: ' FETCHED

SAY ''
SAY 'Step 5: Use VARLOAD to set multiple variables from records'

/* Create records with name-value pairs */
ADDRESS PIPELINE,
  'literal RESULT success |',
  'literal COUNT' remaining.0 '|',
  'literal TIMESTAMP 2026-10-27 |',
  'varload |',
  'hole'

SAY 'Variables loaded by VARLOAD:'
SAY '  RESULT =' RESULT
SAY '  COUNT =' COUNT
SAY '  TIMESTAMP =' TIMESTAMP

SAY ''
SAY 'Step 6: Use REXXVARS to list all variables'
SAY '(showing first 10 only)'

ADDRESS PIPELINE,
  'rexxvars |',
  'take first 10 |',
  'stem input ALLVARS.'

DO i = 1 TO ALLVARS.0
  SAY '  ' ALLVARS.i
END

/* Clean up */
CALL STREAM 'photos_all.txt', 'C', 'CLOSE'
CALL STREAM 'photos_done.txt', 'C', 'CLOSE'
'rm -f photos_all.txt photos_done.txt'

SAY ''
SAY 'Complete. All variable pool stages demonstrated.'
