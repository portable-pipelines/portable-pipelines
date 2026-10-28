//! Built-in Portable Pipelines stages (portable subset of IBM CMS Pipelines).

mod abbrev;
mod aggrc;
mod all;
mod beat;
mod between;
mod builtins;
mod casei;
mod chop;
mod collate;
mod combine;
mod common;
mod dam;
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
mod fillup;
mod find;
mod gate;
mod gather;
mod httpsplit;
mod if_stage;
mod input_range;
mod insert;
mod join;
mod joincont;
mod juxtapose;
mod labels;
mod lookup;
mod merge;
mod mime64;
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
mod stem;
mod strliteral;
mod substring;
mod synchronise;
mod tab;
mod target;
mod threeway;
mod tokenise;
mod unique;
mod update;
mod urldeblock;
mod var;
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
pub use input_range::{
    extract_input_range, locate_input_range, parse_input_range, InputRange, LocatedSpan,
};
pub use registry::{register_builtins, StageRegistry};
pub use run::{
    callpipe_from_spec, invoke_python_hook, run_callpipe_standalone, run_parsed,
    run_parsed_with_return_code, run_parsed_with_trace, run_spec, run_spec_with_return_code,
    run_spec_with_trace, run_spec_with_trace_return_code, set_python_hook, set_rexx_hook,
    set_varpool_hook, PythonHook, RexxHook, VarPoolHook, VarPoolOp, VarPoolResult,
};
