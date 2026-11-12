//! Built-in Portable Pipelines stages (portable subset of IBM CMS Pipelines).

mod abbrev;
mod aggrc;
mod all;
mod beat;
mod between;
mod builtins;
mod casei;
mod chop;
mod civil;
mod collate;
mod combine;
mod common;
mod console_host;
mod crc;
mod csv_stage;
mod dam;
mod dateconvert;
mod deal;
mod deblock;
mod delay;
mod digest;
mod duplicate;
mod elastic;
mod eofback;
mod escape;
mod fanintwo;
mod fanoutwo;
mod file_io;
mod fileback;
mod filerand;
mod fileslow;
mod fileupdate;
mod fillup;
mod find;
mod gate;
mod gather;
mod getfiles;
mod greg;
mod host_cmd;
mod httpsplit;
mod if_stage;
mod input_range;
mod insert;
mod instore;
mod join;
mod joincont;
mod json_row;
mod json_scan;
mod json_stage;
mod juxtapose;
mod labels;
mod lookup;
mod merge;
mod mime64;
mod missing_field;
mod noeofback;
mod not_stage;
mod overlay;
mod pad;
mod parcel;
mod pick;
mod pipcmd;
pub mod pipeline_cmd;
mod pipestop;
mod predselect;
mod preface;
mod quoted_printable;
mod random;
mod registry;
mod reverse;
mod rexx_varpool;
mod rexxvars;
mod run;
mod runpipe;
mod snake;
mod space;
mod spec;
mod spec_expr;
mod spill;
mod split;
mod stack;
mod stem;
mod strliteral;
mod subcom;
mod substring;
mod synchronise;
mod sysvar;
mod tab;
mod target;
mod threeway;
mod timestamp;
mod tokenise;
mod topology;
mod unique;
mod update;
mod urldeblock;
mod var;
mod vardrop;
mod varfetch;
mod varload;
mod varset;
mod verify;
mod wildcard;
mod xlate;
mod xrange;
mod xrange_driver;
mod zone;

pub use builtins::*;
pub use console_host::{with_console_host, ConsoleHost};
pub use input_range::{
    extract_input_range, locate_input_range, parse_input_range, InputRange, LocatedSpan,
};
pub use missing_field::{
    extract_key_noted, extract_range_noted, extract_range_noted_lossy, json_field_tokens,
    missing_field_message, missing_json_field, note_missing_json_field, operand_field_hint,
    suggest_field, MissingJsonField,
};
pub use registry::{register_builtins, StageRegistry};
pub use run::{
    addpipe_from_spec, callpipe_from_spec, debug_session, invoke_python_hook,
    run_callpipe_standalone, run_parsed, run_parsed_with_return_code, run_parsed_with_trace,
    run_spec, run_spec_with_return_code, run_spec_with_trace, run_spec_with_trace_return_code,
    set_python_hook, set_rexx_hook, set_varpool_hook, PythonHook, RexxHook, VarPoolHook, VarPoolOp,
    VarPoolResult,
};
pub use topology::{
    inspect_topology, is_language_stage_verb, NodeKind, PipelineTopology, TopoEdge, TopoNode,
};
