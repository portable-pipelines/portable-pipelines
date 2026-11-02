/* Live CALLPIPE that reads a stem while records stream.
 *
 * STEM runs in the parent (loaned CALLPIPE). Reverse SHV reads COLORS.
 * from the Regina helper. The helper does not drain remaining input.
 *
 * Usage, from the folder that contains pipe / pipe.exe:
 *
 *   pipe "examples/callpipe_stem.rexx | console"
 *
 * Expected output: three lines, ONE then TWO then THREE.
 */
COLORS.0 = 3
COLORS.1 = 'one'
COLORS.2 = 'two'
COLORS.3 = 'three'
'CALLPIPE' 'stem COLORS. | xlate 1-* upper | *:'
