/* PEEKTO then CALLPIPE star. Small Portable Pipelines test input. */
'PEEKTO LINE'
'OUTPUT' 'peek:' || LINE
'CALLPIPE' '*: | xlate 1-* upper | *:'
