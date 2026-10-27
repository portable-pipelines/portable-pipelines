/* Photo file processing workflow using REXX variable pool stages.
 *
 * This example demonstrates loading a list of photo filenames from a file,
 * filtering them with pipeline stages, and acting on the results.
 *
 * Setup: Create /tmp/photos_all.txt with photo filenames, one per line.
 *
 * Usage: pipe "literal dummy | examples/photo_workflow.rexx | console"
 */

SAY 'Photo Workflow Example'
SAY ''

/* Load all photos from file into a stem */
'CALLPIPE < /tmp/photos_all.txt | stem PHOTOS.'
SAY 'Loaded' PHOTOS.0 'photos from /tmp/photos_all.txt'
SAY ''

/* Filter for vacation photos */
SAY 'Finding vacation photos...'
'CALLPIPE stem PHOTOS. | locate /vacation/ | stem VACATION.'
SAY 'Found' VACATION.0 'vacation photos:'
DO I = 1 TO VACATION.0
  SAY '  -' VACATION.I
END
SAY ''

/* Process each photo */
SAY 'Commands to run:'
DO I = 1 TO VACATION.0
  photo = VACATION.I
  output = photo || '.thumb.jpg'
  SAY '  convert' photo '-resize 800x600' output
END

SAY ''
SAY 'Workflow complete'
