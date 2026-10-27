//! Variable pool operations for Regina REXX.

use std::ffi::CString;
use std::os::raw::{c_char, c_uchar};

use crate::regina::{self, RxString, ShvBlock};

const RXSHV_FETCH: c_uchar = 0x01;
const RXSHV_SET: c_uchar = 0x00;
const RXSHV_NEXTV: c_uchar = 0x0B;

const RXSHV_BADN: u8 = 0x08;
const RXSHV_MEMFL: u8 = 0x10;
const RXSHV_BADF: u8 = 0x80;
const RXSHV_LVAR: u8 = 0x02;

/// Get a variable from the REXX variable pool.
pub fn varpool_get(name: &str) -> Result<String, String> {
    if !regina::regina_available() {
        return Err(
            "REXX variable pool access requires Regina and must be called from a REXX program"
                .to_string(),
        );
    }

    let name_upper = name.to_ascii_uppercase();
    let name_c = CString::new(name_upper.as_str()).map_err(|e| e.to_string())?;
    let mut value_buf = vec![0u8; 256];

    let mut shv = ShvBlock {
        shvnext: std::ptr::null_mut(),
        shvname: RxString {
            strlength: name_upper.len() as regina::api::ULONG,
            strptr: name_c.as_ptr() as *mut c_char,
        },
        shvvalue: RxString {
            strlength: value_buf.len() as regina::api::ULONG,
            strptr: value_buf.as_mut_ptr() as *mut c_char,
        },
        shvnamelen: name_upper.len() as regina::api::ULONG,
        shvvaluelen: value_buf.len() as regina::api::ULONG,
        shvcode: RXSHV_FETCH,
        shvret: 0,
    };

    regina::api::with_api(|api_inst| {
            unsafe { (api_inst.variable_pool)(&mut shv) };
        });

    if (shv.shvret & RXSHV_BADN) != 0 {
        return Err(format!("variable {} not found", name));
    }

    if (shv.shvret & (RXSHV_MEMFL | RXSHV_BADF)) != 0 {
        return Err(format!(
            "error fetching variable {}: shvret={}",
            name, shv.shvret
        ));
    }

    let len = shv.shvvalue.strlength as usize;
    if len > value_buf.len() {
        value_buf.resize(len, 0);
        shv.shvvalue.strlength = value_buf.len() as regina::api::ULONG;
        shv.shvvalue.strptr = value_buf.as_mut_ptr() as *mut c_char;
        shv.shvvaluelen = value_buf.len() as regina::api::ULONG;

        regina::api::with_api(|api_inst| {
            unsafe { (api_inst.variable_pool)(&mut shv) };
        });

        if shv.shvret != 0 {
            return Err(format!("error fetching variable {}", name));
        }
    }

    let actual_len = shv.shvvalue.strlength as usize;
    Ok(String::from_utf8_lossy(&value_buf[..actual_len]).into_owned())
}

/// Set a variable in the REXX variable pool.
pub fn varpool_set(name: &str, value: &str) -> Result<(), String> {
    if !regina::regina_available() {
        return Err(
            "REXX variable pool access requires Regina and must be called from a REXX program"
                .to_string(),
        );
    }

    let name_upper = name.to_ascii_uppercase();
    let name_c = CString::new(name_upper.as_str()).map_err(|e| e.to_string())?;
    let mut value_buf = value.as_bytes().to_vec();

    let mut shv = ShvBlock {
        shvnext: std::ptr::null_mut(),
        shvname: RxString {
            strlength: name_upper.len() as regina::api::ULONG,
            strptr: name_c.as_ptr() as *mut c_char,
        },
        shvvalue: RxString {
            strlength: value_buf.len() as regina::api::ULONG,
            strptr: value_buf.as_mut_ptr() as *mut c_char,
        },
        shvnamelen: name_upper.len() as regina::api::ULONG,
        shvvaluelen: value_buf.len() as regina::api::ULONG,
        shvcode: RXSHV_SET,
        shvret: 0,
    };

    regina::api::with_api(|api_inst| {
            unsafe { (api_inst.variable_pool)(&mut shv) };
        });

    drop(name_c);
    drop(value_buf);

    if (shv.shvret & (RXSHV_BADN | RXSHV_MEMFL | RXSHV_BADF)) != 0 {
        return Err(format!(
            "error setting variable {}: shvret={}",
            name, shv.shvret
        ));
    }

    Ok(())
}

/// List all variables in the REXX variable pool.
pub fn varpool_list() -> Result<Vec<(String, String)>, String> {
    if !regina::regina_available() {
        return Err(
            "REXX variable pool access requires Regina and must be called from a REXX program"
                .to_string(),
        );
    }

    let mut results = Vec::new();

    loop {
        let mut name_buf = vec![0u8; 256];
        let mut value_buf = vec![0u8; 256];

        let mut shv = ShvBlock {
            shvnext: std::ptr::null_mut(),
            shvname: RxString {
                strlength: name_buf.len() as regina::api::ULONG,
                strptr: name_buf.as_mut_ptr() as *mut c_char,
            },
            shvvalue: RxString {
                strlength: value_buf.len() as regina::api::ULONG,
                strptr: value_buf.as_mut_ptr() as *mut c_char,
            },
            shvnamelen: name_buf.len() as regina::api::ULONG,
            shvvaluelen: value_buf.len() as regina::api::ULONG,
            shvcode: RXSHV_NEXTV,
            shvret: 0,
        };

        regina::api::with_api(|api_inst| {
            unsafe { (api_inst.variable_pool)(&mut shv) };
        });

        if shv.shvret == RXSHV_LVAR {
            break;
        }

        if (shv.shvret & (RXSHV_BADN | RXSHV_MEMFL | RXSHV_BADF)) != 0 {
            break;
        }

        let name_len = shv.shvname.strlength as usize;
        let value_len = shv.shvvalue.strlength as usize;

        let name = String::from_utf8_lossy(&name_buf[..name_len]).into_owned();
        let value = String::from_utf8_lossy(&value_buf[..value_len]).into_owned();

        results.push((name, value));
    }

    Ok(results)
}
