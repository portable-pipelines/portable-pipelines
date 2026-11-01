/* Stream records through a nested CALLPIPE.
 *
 * The nested pipeline uses `*:` connectors. Records flow one at a time
 * (the helper does not drain remaining input first). Pair this with a
 * large or infinite producer and TAKE so the pipeline can stop early.
 *
 * Usage, from the folder that contains pipe / pipe.exe:
 *
 *   pipe "literal hello world| duplicate 10000 | examples/callpipe_stream.rexx | take 10 | console"
 *
 * Expected output: ten lines of HELLO WORLD.
 */
'CALLPIPE' '*: | xlate 1-* upper | *:'
