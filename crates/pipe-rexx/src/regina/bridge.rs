//! Run a REXX stage through Regina with ADDRESS PIPELINE subcommands.

use std::cell::RefCell;
use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use pipe_core::{PipeError, PipeResult};

use crate::subcommands::RexxPipeHost;

use super::api::{
    self, PRxString, RxString, APIRET, PSHORT, PUSHORT, RXCOMMAND, RXSUBCOM_OK, ULONG,
};
use super::discover;

thread_local! {
    static HOST: RefCell<Option<*mut RexxPipeHost<'static>>> = const { RefCell::new(None) };
}

static SUBCOM_REGISTERED: AtomicBool = AtomicBool::new(false);

/// True if Regina's shared library was found and loaded.
pub fn regina_available() -> bool {
    api::try_load()
}

/// Run `script_path` under Regina with `host` bound to ADDRESS PIPELINE.
pub fn run_with_regina(host: &mut RexxPipeHost<'_>, script_path: &Path) -> PipeResult<()> {
    if !api::try_load() {
        return Err(PipeError::stage(format!(
            "Regina not found ({})",
            discover::discovery_hint()
        )));
    }

    ensure_subcom_registered()?;

    let path_str = script_path
        .to_str()
        .ok_or_else(|| PipeError::stage("REXX script path is not valid UTF-8"))?;
    let path_c = CString::new(path_str)
        .map_err(|_| PipeError::stage("REXX script path contains interior NUL"))?;
    let env_c = CString::new("PIPELINE").unwrap();

    // Extend host lifetime for TLS during RexxStart (synchronous; no escape).
    let host_ptr = host as *mut RexxPipeHost<'_> as *mut RexxPipeHost<'static>;

    let start_rc = HOST.with(|slot| {
        *slot.borrow_mut() = Some(host_ptr);
        let rc = api::with_api(|api| unsafe {
            let mut retcode: i16 = 0;
            let mut result = RxString {
                strlength: 0,
                strptr: std::ptr::null_mut(),
            };
            (api.rexx_start)(
                0,
                std::ptr::null_mut(),
                path_c.as_ptr(),
                std::ptr::null_mut(), // load from file
                env_c.as_ptr(),
                RXCOMMAND,
                std::ptr::null_mut(),
                &mut retcode as PSHORT,
                &mut result,
            )
        });
        *slot.borrow_mut() = None;
        rc
    });

    match start_rc {
        Some(0) => Ok(()),
        Some(code) => Err(PipeError::stage(format!(
            "RexxStart failed with code {code} for {}",
            script_path.display()
        ))),
        None => Err(PipeError::stage("Regina API unavailable")),
    }
}

fn ensure_subcom_registered() -> PipeResult<()> {
    if SUBCOM_REGISTERED.load(Ordering::SeqCst) {
        return Ok(());
    }
    let env = CString::new("PIPELINE").unwrap();
    let rc = api::with_api(|api| unsafe {
        (api.register_subcom)(env.as_ptr(), pipeline_subcom, std::ptr::null_mut())
    })
    .ok_or_else(|| PipeError::stage("Regina API unavailable"))?;

    // 0 = OK, 10 = RXSUBCOM_DUP (already registered)
    if rc != RXSUBCOM_OK && rc != 10 {
        return Err(PipeError::stage(format!(
            "RexxRegisterSubcomExe(PIPELINE) failed rc={rc}"
        )));
    }
    SUBCOM_REGISTERED.store(true, Ordering::SeqCst);
    Ok(())
}

#[cfg(windows)]
unsafe extern "system" fn pipeline_subcom(
    command: PRxString,
    flags: PUSHORT,
    retstr: PRxString,
) -> APIRET {
    unsafe { pipeline_subcom_inner(command, flags, retstr) }
}

#[cfg(not(windows))]
unsafe extern "C" fn pipeline_subcom(
    command: PRxString,
    flags: PUSHORT,
    retstr: PRxString,
) -> APIRET {
    unsafe { pipeline_subcom_inner(command, flags, retstr) }
}

unsafe fn pipeline_subcom_inner(
    command: PRxString,
    flags: PUSHORT,
    retstr: PRxString,
) -> APIRET {
    if !flags.is_null() {
        *flags = 0;
    }

    let cmd = unsafe { rxstring_to_string(command) }.unwrap_or_default();
    let result = HOST.with(|slot| {
        let ptr = *slot.borrow();
        match ptr {
            None => Err("no active PIPELINE host".to_string()),
            Some(p) => {
                let host = unsafe { &mut *p };
                dispatch_pipeline_command(host, &cmd)
            }
        }
    });

    match result {
        Ok(rc) => {
            unsafe { set_retstr(retstr, &rc.to_string()) };
            RXSUBCOM_OK
        }
        Err(_msg) => {
            if !flags.is_null() {
                *flags = 0x01; // error
            }
            unsafe { set_retstr(retstr, "8") };
            RXSUBCOM_OK
        }
    }
}

fn dispatch_pipeline_command(host: &mut RexxPipeHost<'_>, cmd: &str) -> Result<i32, String> {
    let trimmed = cmd.trim();
    if trimmed.is_empty() {
        return Ok(0);
    }
    let (verb, rest) = split_verb(trimmed);
    let upper = verb.to_ascii_uppercase();

    match upper.as_str() {
        "READTO" => {
            let var = rest
                .split_whitespace()
                .next()
                .ok_or_else(|| "READTO needs a variable name".to_string())?;
            match host.readto().map_err(|e| e.to_string())? {
                Some(s) => {
                    api::with_api(|api| api.shv_set(&var.to_ascii_uppercase(), &s))
                        .ok_or_else(|| "Regina API unavailable".to_string())??;
                    Ok(0)
                }
                None => Ok(12),
            }
        }
        "WRITETO" | "OUTPUT" => {
            host.writeto(rest.trim()).map_err(|e| e.to_string())?;
            Ok(0)
        }
        "CALLPIPE" => {
            let spec = strip_quotes(rest.trim());
            host.callpipe(spec).map_err(|e| e.to_string())?;
            Ok(0)
        }
        "ADDPIPE" => {
            let spec = strip_quotes(rest.trim());
            host.addpipe(spec).map_err(|e| e.to_string())?;
            Ok(0)
        }
        "SHORT" => {
            host.short();
            Ok(0)
        }
        "SELECT" => {
            let mut parts = rest.split_whitespace();
            let which = parts.next().unwrap_or("").to_ascii_uppercase();
            let n: u32 = parts
                .next()
                .ok_or_else(|| "SELECT needs INPUT/OUTPUT and a number".to_string())?
                .parse()
                .map_err(|_| "SELECT stream number invalid".to_string())?;
            match which.as_str() {
                "INPUT" => host.select_input(n),
                "OUTPUT" => host.select_output(n),
                _ => return Err(format!("SELECT {which}: use INPUT or OUTPUT")),
            }
            Ok(0)
        }
        "COMMIT" => {
            let level: i32 = rest
                .split_whitespace()
                .next()
                .unwrap_or("0")
                .parse()
                .unwrap_or(0);
            host.commit(level).map_err(|e| e.to_string())?;
            Ok(0)
        }
        _ => Err(format!("unknown PIPELINE command: {trimmed}")),
    }
}

fn split_verb(cmd: &str) -> (&str, &str) {
    let cmd = cmd.trim_start();
    match cmd.find(char::is_whitespace) {
        Some(i) => (&cmd[..i], cmd[i..].trim_start()),
        None => (cmd, ""),
    }
}

fn strip_quotes(s: &str) -> &str {
    let s = s.trim();
    if s.len() >= 2 {
        let b = s.as_bytes();
        if (b[0] == b'\'' && b[s.len() - 1] == b'\'')
            || (b[0] == b'"' && b[s.len() - 1] == b'"')
        {
            return &s[1..s.len() - 1];
        }
    }
    s
}

unsafe fn rxstring_to_string(rx: PRxString) -> Option<String> {
    if rx.is_null() {
        return None;
    }
    let r = &*rx;
    if r.strptr.is_null() {
        return Some(String::new());
    }
    if r.strlength == 0 {
        return CStr::from_ptr(r.strptr)
            .to_str()
            .ok()
            .map(|s| s.to_string());
    }
    let slice = std::slice::from_raw_parts(r.strptr as *const u8, r.strlength as usize);
    Some(String::from_utf8_lossy(slice).into_owned())
}

unsafe fn set_retstr(retstr: PRxString, s: &str) {
    if retstr.is_null() {
        return;
    }
    let r = &mut *retstr;
    if !r.strptr.is_null() && (r.strlength as usize) >= s.len() {
        std::ptr::copy_nonoverlapping(s.as_ptr(), r.strptr as *mut u8, s.len());
        r.strlength = s.len() as ULONG;
        return;
    }
    // Fallback: allocate a durable C string (Regina often frees with its allocator;
    // short RC codes normally fit the provided buffer above).
    if let Ok(c) = CString::new(s) {
        let len = s.len();
        let ptr = c.into_raw();
        r.strptr = ptr as *mut c_char;
        r.strlength = len as ULONG;
    }
}
