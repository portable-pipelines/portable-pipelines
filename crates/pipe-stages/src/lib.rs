//! Built-in CMS Pipelines stages (portable subset).

mod builtins;
mod registry;
mod run;
mod split;

pub use builtins::*;
pub use registry::{register_builtins, StageRegistry};
pub use run::{
    callpipe_from_spec, run_parsed, run_parsed_with_trace, run_spec, run_spec_with_trace,
    set_rexx_hook, RexxHook,
};
