//! REXX variable pool interface for stages.
//!
//! Provides get, set, and list operations on Regina variables when running
//! under a REXX program. Returns clear errors when Regina is not available.

use crate::run::{call_varpool_hook, VarPoolOp, VarPoolResult};

/// Get a variable from the REXX pool.
pub fn get_variable(name: &str) -> Result<String, String> {
    match call_varpool_hook(VarPoolOp::Get(name.to_string()))? {
        VarPoolResult::Value(v) => Ok(v),
        _ => Err("unexpected result from varpool hook".to_string()),
    }
}

/// Set a variable in the REXX pool.
pub fn set_variable(name: &str, value: &str) -> Result<(), String> {
    match call_varpool_hook(VarPoolOp::Set(name.to_string(), value.to_string()))? {
        VarPoolResult::Success => Ok(()),
        _ => Err("unexpected result from varpool hook".to_string()),
    }
}

/// List all variables in the REXX pool.
pub fn list_variables() -> Result<Vec<(String, String)>, String> {
    match call_varpool_hook(VarPoolOp::List)? {
        VarPoolResult::List(v) => Ok(v),
        _ => Err("unexpected result from varpool hook".to_string()),
    }
}

/// Drop a variable (or a stem when the name ends with `.`).
pub fn drop_variable(name: &str) -> Result<(), String> {
    match call_varpool_hook(VarPoolOp::Drop(name.to_string()))? {
        VarPoolResult::Success => Ok(()),
        _ => Err("unexpected result from varpool hook".to_string()),
    }
}
