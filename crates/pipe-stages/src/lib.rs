//! Built-in CMS Pipelines stages (portable subset).

mod builtins;
mod chop;
mod common;
mod dam;
mod duplicate;
mod elastic;
mod gate;
mod input_range;
mod join;
mod joincont;
mod juxtapose;
mod lookup;
mod pick;
mod preface;
mod registry;
mod run;
mod spec;
mod split;
mod unique;
mod xrange;
mod zone;

pub use builtins::*;
pub use registry::{register_builtins, StageRegistry};
pub use run::{
    callpipe_from_spec, run_parsed, run_parsed_with_trace, run_spec, run_spec_with_trace,
    set_rexx_hook, RexxHook,
};
