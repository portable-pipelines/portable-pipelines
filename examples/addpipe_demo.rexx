/* Add a pipeline to the running set, then keep going.
 *
 * ADDPIPE inserts `literal hello` on this stage's input. The caller
 * keeps reading, so READTO sees hello. A later OUTPUT still runs.
 * Two output records. No ELASTIC. The upstream record is not read.
 *
 * Usage, from the folder that contains pipe / pipe.exe:
 *
 *   pipe "literal ignored| examples/addpipe_demo.rexx | console"
 *
 * Windows:
 *
 *   .\pipe.exe "literal ignored| examples\addpipe_demo.rexx | console"
 *
 * Expected output:
 *   hello
 *   still-here
 */
'ADDPIPE' '*: | literal hello'
'READTO LINE'
'OUTPUT' LINE
'OUTPUT' 'still-here'
