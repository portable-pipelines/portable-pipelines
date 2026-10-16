/* Demonstrate CALLPIPE with * connectors.
 *
 * The nested pipeline is fed from this stage's input and drains to its
 * output. Do not READTO first — a leading * consumes the remaining input
 * stream (records already READTO'd are gone).
 *
 * Wraps every record in brackets via SPECS.
 */
'CALLPIPE' '*: | specs /[/ WORD 1 /]/ | *:'
