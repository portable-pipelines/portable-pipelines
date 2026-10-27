//! Dynamically loaded Regina SAA entry points.

use std::ffi::CString;
use std::os::raw::{c_char, c_long, c_short, c_uchar, c_ulong, c_void};
use std::path::Path;
use std::sync::Mutex;

use once_cell::sync::OnceCell;

use super::discover::find_regina_library;

pub type ULONG = c_ulong;
pub type LONG = c_long;
pub type SHORT = c_short;
pub type USHORT = u16;
pub type APIRET = ULONG;
pub type PUCHAR = *mut c_uchar;
pub type PSHORT = *mut SHORT;
pub type PUSHORT = *mut USHORT;
pub type PCSZ = *const c_char;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct RxString {
    pub strlength: ULONG,
    pub strptr: *mut c_char,
}

pub type PRxString = *mut RxString;

#[repr(C)]
pub struct ShvBlock {
    pub shvnext: *mut ShvBlock,
    pub shvname: RxString,
    pub shvvalue: RxString,
    pub shvnamelen: ULONG,
    pub shvvaluelen: ULONG,
    pub shvcode: c_uchar,
    pub shvret: c_uchar,
}

pub const RXCOMMAND: LONG = 0;
pub const RXSUBCOM_OK: APIRET = 0;
pub const RXSHV_SET: c_uchar = 0x00;
/// Informational: variable did not previously exist (still success).
pub const RXSHV_NEWV: APIRET = 0x01;
pub const RXSHV_BADN: u8 = 0x08;
pub const RXSHV_MEMFL: u8 = 0x10;
pub const RXSHV_BADF: u8 = 0x80;

#[cfg(windows)]
pub type SubcomFn = unsafe extern "system" fn(PRxString, PUSHORT, PRxString) -> APIRET;
#[cfg(not(windows))]
pub type SubcomFn = unsafe extern "C" fn(PRxString, PUSHORT, PRxString) -> APIRET;

#[cfg(windows)]
type RexxStartFn = unsafe extern "system" fn(
    LONG,
    PRxString,
    PCSZ,
    PRxString,
    PCSZ,
    LONG,
    *mut c_void,
    PSHORT,
    PRxString,
) -> APIRET;
#[cfg(not(windows))]
type RexxStartFn = unsafe extern "C" fn(
    LONG,
    PRxString,
    PCSZ,
    PRxString,
    PCSZ,
    LONG,
    *mut c_void,
    PSHORT,
    PRxString,
) -> APIRET;

#[cfg(windows)]
type RexxRegisterSubcomExeFn = unsafe extern "system" fn(PCSZ, SubcomFn, PUCHAR) -> APIRET;
#[cfg(not(windows))]
type RexxRegisterSubcomExeFn = unsafe extern "C" fn(PCSZ, SubcomFn, PUCHAR) -> APIRET;

#[cfg(windows)]
type RexxDeregisterSubcomFn = unsafe extern "system" fn(PCSZ, PCSZ) -> APIRET;
#[cfg(not(windows))]
type RexxDeregisterSubcomFn = unsafe extern "C" fn(PCSZ, PCSZ) -> APIRET;

#[cfg(windows)]
type RexxVariablePoolFn = unsafe extern "system" fn(*mut ShvBlock) -> APIRET;
#[cfg(not(windows))]
type RexxVariablePoolFn = unsafe extern "C" fn(*mut ShvBlock) -> APIRET;

pub struct ReginaApi {
    /// Keeps the library mapped for the process lifetime.
    #[cfg(windows)]
    _module: windows_sys::Win32::Foundation::HMODULE,
    #[cfg(unix)]
    _lib: libloading::Library,
    pub rexx_start: RexxStartFn,
    pub register_subcom: RexxRegisterSubcomExeFn,
    #[allow(dead_code)]
    pub deregister_subcom: RexxDeregisterSubcomFn,
    pub variable_pool: RexxVariablePoolFn,
}

// Function pointers + module handle are process-global; we only load once.
unsafe impl Send for ReginaApi {}
unsafe impl Sync for ReginaApi {}

static API: OnceCell<Mutex<Option<ReginaApi>>> = OnceCell::new();

/// Try to load Regina once. Returns `true` if available.
pub fn try_load() -> bool {
    let cell = API.get_or_init(|| Mutex::new(None));
    let mut guard = cell.lock().unwrap();
    if guard.is_some() {
        return true;
    }
    let Some(path) = find_regina_library() else {
        return false;
    };
    match unsafe { load_from_path(&path) } {
        Ok(api) => {
            *guard = Some(api);
            true
        }
        Err(_) => false,
    }
}

/// Run `f` with the loaded API. Must not call back into `with_api` /
/// `try_load` from Regina while `f` still holds the mutex (e.g. do not wrap
/// `RexxStart` in `with_api` — subcommands need the lock for SHV).
pub fn with_api<R>(f: impl FnOnce(&ReginaApi) -> R) -> Option<R> {
    try_load();
    let cell = API.get()?;
    let guard = cell.lock().ok()?;
    Some(f(guard.as_ref()?))
}

#[cfg(windows)]
unsafe fn load_from_path(path: &Path) -> Result<ReginaApi, String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};

    let wide: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let module = LoadLibraryW(wide.as_ptr());
    if module.is_null() {
        return Err(format!("LoadLibraryW failed for {}", path.display()));
    }

    unsafe fn sym<T>(module: windows_sys::Win32::Foundation::HMODULE, name: &[u8]) -> Result<T, String> {
        let p = GetProcAddress(module, name.as_ptr() as _);
        if p.is_none() {
            return Err(format!(
                "GetProcAddress({}) failed",
                String::from_utf8_lossy(name)
            ));
        }
        Ok(std::mem::transmute_copy(&p))
    }

    Ok(ReginaApi {
        _module: module,
        rexx_start: sym(module, b"RexxStart\0")?,
        register_subcom: sym(module, b"RexxRegisterSubcomExe\0")?,
        deregister_subcom: sym(module, b"RexxDeregisterSubcom\0")?,
        variable_pool: sym(module, b"RexxVariablePool\0")?,
    })
}

#[cfg(unix)]
unsafe fn load_from_path(path: &Path) -> Result<ReginaApi, String> {
    let lib = libloading::Library::new(path)
        .map_err(|e| format!("load {}: {e}", path.display()))?;
    let rexx_start = *lib
        .get::<RexxStartFn>(b"RexxStart\0")
        .map_err(|e| format!("RexxStart: {e}"))?;
    let register_subcom = *lib
        .get::<RexxRegisterSubcomExeFn>(b"RexxRegisterSubcomExe\0")
        .map_err(|e| format!("RexxRegisterSubcomExe: {e}"))?;
    let deregister_subcom = *lib
        .get::<RexxDeregisterSubcomFn>(b"RexxDeregisterSubcom\0")
        .map_err(|e| format!("RexxDeregisterSubcom: {e}"))?;
    let variable_pool = *lib
        .get::<RexxVariablePoolFn>(b"RexxVariablePool\0")
        .map_err(|e| format!("RexxVariablePool: {e}"))?;
    Ok(ReginaApi {
        _lib: lib,
        rexx_start,
        register_subcom,
        deregister_subcom,
        variable_pool,
    })
}

impl ReginaApi {
    pub fn shv_set(&self, name: &str, value: &str) -> Result<(), String> {
        let name_c = CString::new(name).map_err(|e| e.to_string())?;
        let mut value_buf = value.as_bytes().to_vec();
        let mut block = ShvBlock {
            shvnext: std::ptr::null_mut(),
            shvname: RxString {
                strlength: name.len() as ULONG,
                strptr: name_c.as_ptr() as *mut c_char,
            },
            shvvalue: RxString {
                strlength: value.len() as ULONG,
                strptr: value_buf.as_mut_ptr() as *mut c_char,
            },
            shvnamelen: name.len() as ULONG,
            shvvaluelen: value.len() as ULONG,
            shvcode: RXSHV_SET,
            shvret: 0,
        };
        // Regina may return soft status bits (e.g. NEWV) as the APIRET.
        let rc = unsafe { (self.variable_pool)(&mut block) };
        let soft = rc == 0 || rc == RXSHV_NEWV;
        let hard = block.shvret & (RXSHV_BADN | RXSHV_MEMFL | RXSHV_BADF);
        drop(name_c);
        drop(value_buf);
        if !soft || hard != 0 {
            return Err(format!(
                "RexxVariablePool rc={rc} shvret={}",
                block.shvret
            ));
        }
        Ok(())
    }
}

/// Return the Regina variable pool function pointer for direct use by stages.
pub fn regina_varpool_fn() -> Option<unsafe fn(*mut std::ffi::c_void)> {
    try_load();
    let cell = API.get()?;
    let guard = cell.lock().ok()?;
    let api = guard.as_ref()?;
    // Cast the typed function pointer to a generic one.
    Some(unsafe { std::mem::transmute(api.variable_pool) })
}
