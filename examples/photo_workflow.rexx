/* Photo file processing workflow using REXX variable pool stages.
 *
 * This example demonstrates loading lists from files into stems, filtering
 * with pipeline stages, and acting on results - a real-world workflow.
 *
 * Note: LOOKUP requires multi-pipeline syntax that is not currently supported
 * in CALLPIPE from REXX. This example uses LOCATE as a workaround. For full
 * LOOKUP functionality, see crates/pipe-stages/tests/phase4_lookup.rs.
 *
 * Setup: Create /tmp/photos_all.txt with all photos, one per line.
 *
 * Usage: pipe "literal dummy | examples/photo_workflow.rexx | console"
 */

SAY 'Photo Workflow Example'
SAY ''

/* Load all photos from file into a stem */
'CALLPIPE < /tmp/photos_all.txt | stem PHOTOS.'
SAY 'Total photos:' PHOTOS.0

/* Example: Filter for vacation photos using LOCATE */
SAY ''
SAY 'Finding vacation photos...'
'CALLPIPE stem PHOTOS. | locate /vacation/ | stem VACATION.'
SAY 'Found' VACATION.0 'vacation photos:'
DO I = 1 TO VACATION.0
  SAY '  -' VACATION.I
END

/* Process each vacation photo */
SAY ''
SAY 'Commands to run:'
DO I = 1 TO VACATION.0
  photo = VACATION.I
  output = photo || '.thumb.jpg'
  SAY '  convert' photo '-resize 800x600' output
END

SAY ''
SAY 'Workflow complete'
