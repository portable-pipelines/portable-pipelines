/* Photo file processing workflow using REXX variable pool stages.
 *
 * This example demonstrates loading a list of photo filenames from a file
 * into a REXX stem, filtering with pipeline stages, and acting on results.
 *
 * Setup: Create /tmp/photos.txt with photo filenames
 *
 * Usage: pipe "literal dummy | examples/photo_workflow.rexx | console"
 */

SAY 'Photo Workflow Example'
SAY ''

/* Step 1: Load all photos from file into stem */
'CALLPIPE < /tmp/photos.txt | stem PHOTOS.'
SAY 'Loaded' PHOTOS.0 'photos from file'
SAY ''

/* Step 2: Filter for vacation photos */
SAY 'Filtering for vacation photos...'
'CALLPIPE stem PHOTOS. | locate /vacation/ | stem VACATION.'
SAY 'Found' VACATION.0 'vacation photos:'
DO I = 1 TO VACATION.0
  SAY '  -' VACATION.I
END
SAY ''

/* Step 3: Process each vacation photo */
SAY 'Would process these photos:'
DO I = 1 TO VACATION.0
  photo = VACATION.I
  SAY '  convert' photo '-> thumb_'photo
END

SAY ''
SAY 'Workflow complete'
