//! Built-in CMS Pipelines stages (portable subset).

mod aggrc;
mod beat;
mod between;
mod builtins;
mod casei;
mod chop;
mod combine;
mod common;
mod dam;
mod delay;
mod duplicate;
mod elastic;
mod eofback;
mod find;
mod gate;
mod if_stage;
mod input_range;
mod insert;
mod join;
mod joincont;
mod juxtapose;
mod labels;
mod lookup;
mod noeofback;
mod not_stage;
mod overlay;
mod pad;
mod pick;
mod pipcmd;
mod pipestop;
mod predselect;
mod preface;
mod registry;
mod reverse;
mod run;
mod runpipe;
mod snake;
mod space;
mod spec;
mod split;
mod synchronise;
mod tokenise;
mod unique;
mod update;
mod xlate;
mod xrange;
mod zone;

pub use builtins::*;
pub use registry::{register_builtins, StageRegistry};
pub use run::{
    callpipe_from_spec, run_parsed, run_parsed_with_return_code, run_parsed_with_trace, run_spec,
    run_spec_with_return_code, run_spec_with_trace, run_spec_with_trace_return_code, set_rexx_hook,
    RexxHook,
};
