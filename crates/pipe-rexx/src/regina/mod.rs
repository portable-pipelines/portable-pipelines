//! Optional Regina REXX runtime (dynamically loaded).
//!
//! If a Regina shared library is found on the machine, `.rexx` / `.rex` stages run through
//! it with ADDRESS `PIPELINE` for our subcommands. Otherwise callers fall back
//! to the embedded subset interpreter.

pub(crate) mod api;
mod bridge;
mod discover;

pub use api::{RxString, ShvBlock};
pub use bridge::{in_regina_run, regina_available, run_with_regina};
pub use discover::discovery_hint;

/// Provide access to Regina variable pool API for stages.
/// Returns a function pointer that can call RexxVariablePool.
pub fn regina_api_for_stages() -> Option<unsafe fn(*mut std::ffi::c_void)> {
    api::regina_varpool_fn()
}
