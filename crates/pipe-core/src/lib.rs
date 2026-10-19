//! CMS Pipelines-style record dispatcher.
//!
//! Stages never talk to each other. They call [`StageCtx`] services; the
//! dispatcher moves at most one record per connection and schedules stages
//! as stackful coroutines (`corosensei`), matching Hartmann's design.

mod dispatcher;
mod error;
mod record;
mod stage;
mod trace;

pub use dispatcher::{
    CallPipeConnectors, ConnectionSpec, Dispatcher, PipelineBuilder, StageId, StageSpec,
};
pub use error::{PipeError, PipeResult};
pub use record::{Record, StreamId};
pub use stage::{StageCtx, StageFn, WaitReason};
pub use trace::{TraceConfig, TraceSink};

/// Default commit level used when a stage has not committed yet.
pub const COMMIT_INITIAL: i32 = -1;

/// Aggregate CMS/TSO Pipelines return codes (Author's Edition 1.1.11
/// pp.39, 219, 258): if any code is negative, the result is the most
/// negative; otherwise it is the largest. No codes yields 0.
pub fn aggregate_return_codes<I>(codes: I) -> i32
where
    I: IntoIterator<Item = i32>,
{
    let mut iter = codes.into_iter();
    let Some(first) = iter.next() else {
        return 0;
    };
    let mut min = first;
    let mut max = first;
    let mut any_neg = first < 0;
    for c in iter {
        if c < 0 {
            any_neg = true;
        }
        min = min.min(c);
        max = max.max(c);
    }
    if any_neg {
        min
    } else {
        max
    }
}

#[cfg(test)]
mod aggregate_rc_tests {
    use super::aggregate_return_codes;

    #[test]
    fn empty_is_zero() {
        assert_eq!(aggregate_return_codes([]), 0);
    }

    #[test]
    fn largest_when_all_nonnegative() {
        // Book p.258: `0 99 3 6` → 99.
        assert_eq!(aggregate_return_codes([0, 99, 3, 6]), 99);
        assert_eq!(aggregate_return_codes([0]), 0);
        assert_eq!(aggregate_return_codes([1, 1, 1]), 1);
    }

    #[test]
    fn most_negative_when_any_negative() {
        // Book p.258: `0 99 -3 6` → -3.
        assert_eq!(aggregate_return_codes([0, 99, -3, 6]), -3);
        assert_eq!(aggregate_return_codes([-1, -10, 5]), -10);
        assert_eq!(aggregate_return_codes([-3, -1]), -3);
        assert_eq!(aggregate_return_codes([i32::MIN, i32::MAX]), i32::MIN);
    }
}
