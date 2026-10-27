/* Photo file processing workflow using REXX variable pool stages.
 *
 * This example demonstrates using STEM and other variable pool stages
 * to process lists of files from within a REXX program running as a
 * pipeline stage.
 *
 * Usage: pipe "literal dummy | examples/photo_workflow.rexx | console"
 */

SAY 'Photo Workflow Example'
SAY ''

/* Populate a stem with photo filenames */
PHOTOS.0 = 5
PHOTOS.1 = 'vacation/IMG_001.jpg'
PHOTOS.2 = 'vacation/IMG_002.png'
PHOTOS.3 = 'family/photo.jpg'
PHOTOS.4 = 'work/diagram.png'
PHOTOS.5 = 'misc/scan.JPG'

SAY 'Processing' PHOTOS.0 'photos...'
SAY ''

/* Filter for JPG files only (case-insensitive) */
SAY 'JPG files:'
'CALLPIPE stem PHOTOS. | locate /\.jpg/i | console'
SAY ''

/* Store result back for further processing */
'CALLPIPE stem PHOTOS. | locate /vacation/ | varset VACATION_COUNT /found/'
SAY 'Vacation folder search:' VACATION_COUNT

SAY ''
SAY 'Workflow complete'
