//! Built-in CMS Pipelines stages (portable subset).

mod builtins;
mod registry;
mod run;

pub use builtins::*;
pub use registry::{register_builtins, StageRegistry};
pub use run::{
    callpipe_from_spec, run_parsed, run_spec, set_rexx_hook, RexxHook,
};
