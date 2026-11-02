/* SELECT by name. PRIMARY and SECONDARY are reserved stream identifiers.
   Pair with a label so the secondary output is connected:
     pipe "(end ?) r: examples/named_streams.rexx | console ? r: | console"
*/

'SELECT OUTPUT PRIMARY'
'OUTPUT' 'even'
'SELECT OUTPUT SECONDARY'
'OUTPUT' 'odd'
