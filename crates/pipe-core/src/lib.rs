//! CMS Pipelines-style record dispatcher.
//!
//! Stages never talk to each other. They call [`StageCtx`] services; the
//! dispatcher moves at most one record per connection and schedules stages
//! as stackful coroutines (`corosensei`), matching Hartmann's design.

mod dispatcher;
mod error;
mod record;
mod stage;

pub use dispatcher::{
    CallPipeConnectors, ConnectionSpec, Dispatcher, PipelineBuilder, StageId, StageSpec,
};
pub use error::{PipeError, PipeResult};
pub use record::{Record, StreamId};
pub use stage::{StageCtx, StageFn, WaitReason};

/// Default commit level used when a stage has not committed yet.
pub const COMMIT_INITIAL: i32 = -1;
