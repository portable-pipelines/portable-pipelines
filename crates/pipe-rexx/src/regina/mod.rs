//! Optional Regina REXX runtime (dynamically loaded).
//!
//! If a Regina shared library is found on the machine, `.rex` stages run through
//! it with ADDRESS `PIPELINE` for our subcommands. Otherwise callers fall back
//! to the embedded subset interpreter.

mod api;
mod bridge;
mod discover;

pub use bridge::{regina_available, run_with_regina};
pub use discover::discovery_hint;
