//! Variable pool operations for Regina REXX.

use std::ffi::CString;
use std::os::raw::c_char;

use crate::regina::{
    self,
    api::{
        RXSHV_BADF, RXSHV_BADN, RXSHV_DROPV, RXSHV_FETCH, RXSHV_LVAR, RXSHV_MEMFL, RXSHV_NEWV,
        RXSHV_NEXTV, RXSHV_SET, RXSHV_TRUNC,
    },
    RxString, ShvBlock,
};

fn not_in_rexx_program() -> String {
    "REXX variable pool access requires Regina and must be called from a REXX program".to_string()
}

fn hard_shv_error(name: &str, op: &str, shvret: u8) -> String {
    if (shvret & RXSHV_BADF) != 0 {
        return not_in_rexx_program();
    }
    format!("error {op} variable {name}: shvret={shvret}")
}

fn shv_call(shv: &mut ShvBlock) -> Result<crate::regina::api::APIRET, String> {
    crate::regina::api::with_api(|api_inst| unsafe { (api_inst.variable_pool)(shv) })
        .ok_or_else(|| "Regina API unavailable".to_string())
}

/// Get a variable from the REXX variable pool.
pub fn varpool_get(name: &str) -> Result<String, String> {
    if !regina::regina_available() {
        return Err(not_in_rexx_program());
    }

    let name_upper = name.to_ascii_uppercase();
    let name_c = CString::new(name_upper.as_str()).map_err(|e| e.to_string())?;
    let mut value_buf = vec![0u8; 256];

    loop {
        let mut shv = ShvBlock {
            shvnext: std::ptr::null_mut(),
            shvname: RxString {
                strlength: name_upper.len() as crate::regina::api::ULONG,
                strptr: name_c.as_ptr() as *mut c_char,
            },
            shvvalue: RxString {
                strlength: value_buf.len() as crate::regina::api::ULONG,
                strptr: value_buf.as_mut_ptr() as *mut c_char,
            },
            shvnamelen: name_upper.len() as crate::regina::api::ULONG,
            shvvaluelen: value_buf.len() as crate::regina::api::ULONG,
            shvcode: RXSHV_FETCH,
            shvret: 0,
        };

        let rc = shv_call(&mut shv)?;
        let soft = rc == 0 || rc == RXSHV_NEWV;
        if (shv.shvret & (RXSHV_BADN | RXSHV_MEMFL | RXSHV_BADF)) != 0 || !soft {
            return Err(hard_shv_error(&name_upper, "fetching", shv.shvret));
        }

        let needed = (shv.shvvalue.strlength as usize).max(shv.shvvaluelen as usize);
        if (shv.shvret & RXSHV_TRUNC) != 0 || needed > value_buf.len() {
            let grow = needed.max(value_buf.len().saturating_mul(2)).max(1);
            value_buf.resize(grow, 0);
            continue;
        }

        if rc == RXSHV_NEWV || (shv.shvret & (RXSHV_NEWV as u8)) != 0 {
            return Ok(String::new());
        }

        let actual_len = shv.shvvalue.strlength as usize;
        let actual_len = actual_len.min(value_buf.len());
        return Ok(String::from_utf8_lossy(&value_buf[..actual_len]).into_owned());
    }
}

/// Set a variable in the REXX variable pool.
pub fn varpool_set(name: &str, value: &str) -> Result<(), String> {
    if !regina::regina_available() {
        return Err(not_in_rexx_program());
    }

    let name_upper = name.to_ascii_uppercase();
    let name_c = CString::new(name_upper.as_str()).map_err(|e| e.to_string())?;
    let mut value_buf = value.as_bytes().to_vec();

    let mut shv = ShvBlock {
        shvnext: std::ptr::null_mut(),
        shvname: RxString {
            strlength: name_upper.len() as crate::regina::api::ULONG,
            strptr: name_c.as_ptr() as *mut c_char,
        },
        shvvalue: RxString {
            strlength: value_buf.len() as crate::regina::api::ULONG,
            strptr: value_buf.as_mut_ptr() as *mut c_char,
        },
        shvnamelen: name_upper.len() as crate::regina::api::ULONG,
        shvvaluelen: value_buf.len() as crate::regina::api::ULONG,
        shvcode: RXSHV_SET,
        shvret: 0,
    };

    let rc = shv_call(&mut shv)?;
    drop(name_c);
    drop(value_buf);

    let soft = rc == 0 || rc == RXSHV_NEWV;
    if (shv.shvret & (RXSHV_BADN | RXSHV_MEMFL | RXSHV_BADF)) != 0 || !soft {
        return Err(hard_shv_error(&name_upper, "setting", shv.shvret));
    }

    Ok(())
}

/// List all variables in the REXX variable pool.
pub fn varpool_list() -> Result<Vec<(String, String)>, String> {
    if !regina::regina_available() {
        return Err(not_in_rexx_program());
    }

    let mut results = Vec::new();
    // NEXTV advances even when truncated, so retrying with a bigger buffer
    // would skip the truncated variable. Use a large buffer and keep what fits.
    let name_cap = 4096usize;
    let value_cap = 65536usize;

    loop {
        let mut name_buf = vec![0u8; name_cap];
        let mut value_buf = vec![0u8; value_cap];

        let mut shv = ShvBlock {
            shvnext: std::ptr::null_mut(),
            shvname: RxString {
                strlength: name_buf.len() as crate::regina::api::ULONG,
                strptr: name_buf.as_mut_ptr() as *mut c_char,
            },
            shvvalue: RxString {
                strlength: value_buf.len() as crate::regina::api::ULONG,
                strptr: value_buf.as_mut_ptr() as *mut c_char,
            },
            shvnamelen: name_buf.len() as crate::regina::api::ULONG,
            shvvaluelen: value_buf.len() as crate::regina::api::ULONG,
            shvcode: RXSHV_NEXTV,
            shvret: 0,
        };

        let _rc = shv_call(&mut shv)?;
        if (shv.shvret & (RXSHV_BADN | RXSHV_MEMFL | RXSHV_BADF)) != 0 {
            if (shv.shvret & RXSHV_BADF) != 0 {
                return Err(not_in_rexx_program());
            }
            break;
        }

        let name_len = (shv.shvname.strlength as usize).min(name_buf.len());
        let value_len = (shv.shvvalue.strlength as usize).min(value_buf.len());
        if name_len == 0 {
            if (shv.shvret & RXSHV_LVAR) != 0 {
                break;
            }
            continue;
        }

        let name = String::from_utf8_lossy(&name_buf[..name_len]).into_owned();
        let value = String::from_utf8_lossy(&value_buf[..value_len]).into_owned();
        results.push((name, value));

        if (shv.shvret & RXSHV_LVAR) != 0 {
            break;
        }
        if results.len() > 100_000 {
            break;
        }
    }

    Ok(results)
}

/// Drop a variable, or a stem when `name` ends with `.`.
pub fn varpool_drop(name: &str) -> Result<(), String> {
    if !regina::regina_available() {
        return Err(not_in_rexx_program());
    }

    let name_upper = name.to_ascii_uppercase();
    if name_upper.ends_with('.') {
        let vars = varpool_list()?;
        for (n, _) in vars {
            if n.to_ascii_uppercase().starts_with(&name_upper) {
                drop_one(&n)?;
            }
        }
    }
    drop_one(&name_upper)
}

fn drop_one(name: &str) -> Result<(), String> {
    let name_c = CString::new(name).map_err(|e| e.to_string())?;
    let mut shv = ShvBlock {
        shvnext: std::ptr::null_mut(),
        shvname: RxString {
            strlength: name.len() as crate::regina::api::ULONG,
            strptr: name_c.as_ptr() as *mut c_char,
        },
        shvvalue: RxString {
            strlength: 0,
            strptr: std::ptr::null_mut(),
        },
        shvnamelen: name.len() as crate::regina::api::ULONG,
        shvvaluelen: 0,
        shvcode: RXSHV_DROPV,
        shvret: 0,
    };

    let rc = shv_call(&mut shv)?;
    drop(name_c);

    let soft = rc == 0 || rc == RXSHV_NEWV;
    if (shv.shvret & (RXSHV_BADN | RXSHV_MEMFL | RXSHV_BADF)) != 0 || !soft {
        return Err(hard_shv_error(name, "dropping", shv.shvret));
    }
    Ok(())
}
