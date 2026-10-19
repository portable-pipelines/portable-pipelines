//! Built-in CMS Pipelines stages (portable subset).

mod aggrc;
mod beat;
mod builtins;
mod chop;
mod common;
mod dam;
mod delay;
mod duplicate;
mod elastic;
mod eofback;
mod gate;
mod if_stage;
mod input_range;
mod join;
mod joincont;
mod juxtapose;
mod lookup;
mod noeofback;
mod pick;
mod predselect;
mod preface;
mod registry;
mod run;
mod spec;
mod split;
mod synchronise;
mod unique;
mod xrange;
mod zone;

pub use builtins::*;
pub use registry::{register_builtins, StageRegistry};
pub use run::{
    callpipe_from_spec, run_parsed, run_parsed_with_return_code, run_parsed_with_trace, run_spec,
    run_spec_with_return_code, run_spec_with_trace, run_spec_with_trace_return_code, set_rexx_hook,
    RexxHook,
};
