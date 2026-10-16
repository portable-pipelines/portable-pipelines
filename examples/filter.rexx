/* Pass records that contain ARG.1 (case-sensitive substring via POS).
 *
 *   pipe "examples/emit.rexx apple banana apricot | examples/filter.rexx ap | console"
 *   → apple / apricot
 *
 * If ARG.1 is omitted, every record passes.
 */
NEEDLE = ARG.1
DO FOREVER
  READTO LINE
  IF RC = 12 THEN LEAVE
  HIT = POS(NEEDLE, LINE)
  PASS = HIT
  IF NEEDLE = '' THEN PASS = 1
  IF PASS <> 0 THEN OUTPUT LINE
END
