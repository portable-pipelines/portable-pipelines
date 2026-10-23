/* Uppercase each input record (Portable Pipelines REXX stage example).
 *
 * Quote pipeline subcommands so Regina does not substitute variables into
 * the command string (IBM style: 'READTO LINE' / 'OUTPUT' expr).
 */
DO FOREVER
  'READTO LINE'
  IF RC = 12 THEN LEAVE
  'OUTPUT' TRANSLATE(LINE)
END
