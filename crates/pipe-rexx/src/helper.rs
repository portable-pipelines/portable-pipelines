//! Hidden `pipe --internal-regina-helper` process that hosts Regina.

use std::fs::File;
use std::io;
use std::path::Path;
use std::process::Command;

#[cfg(test)]
thread_local! {
    static CRASH_NEXT_HELPER: std::cell::Cell<bool> = std::cell::Cell::new(false);
}

/// Test-only: the next helper spawn in this thread aborts immediately.
#[cfg(test)]
pub(crate) fn crash_next_helper() {
    CRASH_NEXT_HELPER.with(|c| c.set(true));
}

use pipe_core::{EofReport, PipeError, PipeResult, StageCtx};
use pipe_helper::protocol::{
    decode_hello, encode_hello_with_meta, read_frame, write_frame, HelloMeta, FAIL, HELLO, HELLO_OK,
};
use pipe_helper::{drive, spawn_piped, SpawnOpts, REGINA_HELPER_ARG};

use crate::protocol_io::ProtocolConn;
use crate::regina;
use crate::subcommands::RexxPipeHost;

const KIND: &str = "REXX helper";

/// Unix fd the parent places a clone of its stdout on (via `pre_exec` dup2).
#[cfg(unix)]
const USER_STDOUT_CHILD_FD: i32 = 3;
#[cfg(unix)]
const USER_STDOUT_FD_ENV: &str = "PIPE_HELPER_USER_STDOUT_FD";
#[cfg(windows)]
const USER_STDOUT_HANDLE_ENV: &str = "PIPE_HELPER_USER_STDOUT_HANDLE";

/// Keeps the parent's stdout clone open until after `spawn`.
struct InheritedStdout {
    #[cfg(unix)]
    _keep: Option<std::os::fd::OwnedFd>,
    #[cfg(windows)]
    _keep: Option<WinHandleGuard>,
}

#[cfg(windows)]
struct WinHandleGuard(isize);

#[cfg(windows)]
impl Drop for WinHandleGuard {
    fn drop(&mut self) {
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(self.0 as _);
        }
    }
}

#[ctor::ctor]
fn auto_enter_helper() {
    enter_if_requested();
}

/// If argv[1] is the hidden helper flag, host Regina and exit.
pub fn enter_if_requested() {
    let mut args = std::env::args();
    let _exe = args.next();
    match args.next().as_deref() {
        Some(a) if a == REGINA_HELPER_ARG => {}
        _ => return,
    }
    if args.next().as_deref() == Some("crash") {
        std::process::abort();
    }
    let code = match run_helper() {
        Ok(()) => 0,
        Err(_) => 1,
    };
    std::process::exit(code);
}

pub fn regina_inproc_requested() -> bool {
    matches!(std::env::var("PIPE_REGINA_INPROC").as_deref(), Ok("1"))
}

fn eof_to_u8(mode: EofReport) -> u8 {
    match mode {
        EofReport::Off => 0,
        EofReport::Current => 1,
        EofReport::Any => 2,
        EofReport::All => 3,
    }
}

pub fn run_rexx_helper(ctx: &mut StageCtx, path: &Path, stage_args: &[String]) -> PipeResult<()> {
    reject_secondary_streams(ctx)?;
    let exe = std::env::current_exe()
        .map_err(|e| PipeError::stage(format!("could not locate this pipe executable: {e}")))?;
    let mut cmd = Command::new(&exe);
    cmd.arg(REGINA_HELPER_ARG);
    #[cfg(test)]
    if CRASH_NEXT_HELPER.with(|c| c.replace(false)) {
        cmd.arg("crash");
    }
    // Protocol uses the child's stdin/stdout. Clone this process's stdout
    // into the helper so Regina SAY and nested CONSOLE still write where
    // in-process Regina wrote (the pipe process stdout).
    let _user_stdout = inherit_parent_stdout(&mut cmd)
        .map_err(|e| PipeError::stage(format!("could not inherit stdout for {KIND}: {e}")))?;
    let mut child = spawn_piped(
        &mut cmd,
        SpawnOpts {
            create_no_window: true,
        },
        KIND,
    )?;
    let meta = HelloMeta {
        stage_num: ctx.stage_id().0 as u32 + 1,
        eof_report: eof_to_u8(ctx.eof_report()),
        nocommit: ctx.nocommit(),
    };
    let hello = encode_hello_with_meta(&path.to_string_lossy(), stage_args, Some(&meta));
    match drive(ctx, &mut child, KIND, &hello) {
        Ok(()) => {
            let _ = child.wait_exit();
            Ok(())
        }
        Err(e) => {
            child.terminate();
            Err(e)
        }
    }
}

fn reject_secondary_streams(ctx: &StageCtx) -> PipeResult<()> {
    let extra_in = ctx.connected_inputs().iter().any(|s| s.0 > 0);
    let extra_out = ctx.connected_outputs().iter().any(|s| s.0 > 0);
    if extra_in || extra_out {
        return Err(PipeError::stage(
            "Regina helper stages are single-stream for now; a label connected a secondary stream",
        ));
    }
    Ok(())
}

fn run_helper() -> Result<(), ()> {
    let (mut proto_in, mut proto_out) = match steal_protocol_stdio() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("REXX helper: {e}");
            return Err(());
        }
    };

    let fail = |out: &mut File, msg: &str| {
        let _ = write_frame(out, FAIL, msg.as_bytes());
    };

    let hello = match read_frame(&mut proto_in) {
        Ok(f) => f,
        Err(e) => {
            fail(&mut proto_out, &format!("helper HELLO read: {e}"));
            return Err(());
        }
    };
    if hello.typ != HELLO {
        fail(
            &mut proto_out,
            &format!("expected HELLO, got type 0x{:02x}", hello.typ),
        );
        return Err(());
    }
    let (path, args, meta) = match decode_hello(&hello.payload) {
        Ok(v) => v,
        Err(e) => {
            fail(&mut proto_out, &format!("hello payload: {e}"));
            return Err(());
        }
    };
    if write_frame(&mut proto_out, HELLO_OK, &[]).is_err() {
        return Err(());
    }

    crate::install_hook();

    let conn = ProtocolConn::from_files(proto_in, proto_out);
    let mut host = RexxPipeHost::from_protocol(conn, meta);
    let script = Path::new(&path);
    match regina::run_with_regina(&mut host, script, &args) {
        Ok(exit_rc) => {
            let rc = host.stage_exit_rc(exit_rc);
            let _ = host.send_exit(rc);
            Ok(())
        }
        Err(e) => {
            let _ = host.send_fail(&e.to_string());
            Err(())
        }
    }
}

fn inherit_parent_stdout(cmd: &mut Command) -> io::Result<InheritedStdout> {
    #[cfg(unix)]
    {
        use std::os::fd::{AsFd, AsRawFd};
        use std::os::unix::process::CommandExt;
        let keep = std::io::stdout().as_fd().try_clone_to_owned()?;
        let src = keep.as_raw_fd();
        unsafe {
            cmd.pre_exec(move || {
                extern "C" {
                    fn dup2(oldfd: i32, newfd: i32) -> i32;
                    fn fcntl(fd: i32, cmd: i32, arg: i32) -> i32;
                }
                const F_SETFD: i32 = 2;
                // dup2(fd, fd) is a no-op and keeps FD_CLOEXEC. The stdout
                // clone is often fd 3, so we must clear CLOEXEC or exec
                // drops the only copy of the parent stdout.
                if dup2(src, USER_STDOUT_CHILD_FD) < 0 {
                    return Err(io::Error::last_os_error());
                }
                if fcntl(USER_STDOUT_CHILD_FD, F_SETFD, 0) < 0 {
                    return Err(io::Error::last_os_error());
                }
                Ok(())
            });
        }
        cmd.env(USER_STDOUT_FD_ENV, USER_STDOUT_CHILD_FD.to_string());
        Ok(InheritedStdout { _keep: Some(keep) })
    }
    #[cfg(windows)]
    {
        use windows_sys::Win32::Foundation::{DuplicateHandle, HANDLE, INVALID_HANDLE_VALUE};
        use windows_sys::Win32::System::Console::{GetStdHandle, STD_OUTPUT_HANDLE};
        use windows_sys::Win32::System::Threading::GetCurrentProcess;
        const DUPLICATE_SAME_ACCESS: u32 = 0x0000_0002;
        unsafe {
            let src = GetStdHandle(STD_OUTPUT_HANDLE);
            if src.is_null() || src == INVALID_HANDLE_VALUE {
                return Err(io::Error::new(
                    io::ErrorKind::Other,
                    "parent stdout handle missing",
                ));
            }
            let proc = GetCurrentProcess();
            let mut dst: HANDLE = std::ptr::null_mut();
            let ok = DuplicateHandle(proc, src, proc, &mut dst, 0, 1, DUPLICATE_SAME_ACCESS);
            if ok == 0 {
                return Err(io::Error::last_os_error());
            }
            cmd.env(USER_STDOUT_HANDLE_ENV, (dst as usize).to_string());
            Ok(InheritedStdout {
                _keep: Some(WinHandleGuard(dst as isize)),
            })
        }
    }
}

fn steal_protocol_stdio() -> io::Result<(File, File)> {
    #[cfg(unix)]
    {
        unix_steal()
    }
    #[cfg(windows)]
    {
        windows_steal()
    }
}

#[cfg(unix)]
fn unix_steal() -> io::Result<(File, File)> {
    use std::os::fd::{AsFd, AsRawFd, OwnedFd};

    extern "C" {
        fn dup2(oldfd: i32, newfd: i32) -> i32;
    }

    let proto_in: OwnedFd = std::io::stdin().as_fd().try_clone_to_owned()?;
    let proto_out: OwnedFd = std::io::stdout().as_fd().try_clone_to_owned()?;
    let user_out = std::env::var(USER_STDOUT_FD_ENV)
        .ok()
        .and_then(|s| s.parse::<i32>().ok());
    std::env::remove_var(USER_STDOUT_FD_ENV);
    let dest_out = user_out.unwrap_or(2);
    unsafe {
        if dup2(dest_out, 1) < 0 {
            return Err(io::Error::last_os_error());
        }
        if dest_out != 1 && dest_out != 2 {
            extern "C" {
                fn close(fd: i32) -> i32;
            }
            let _ = close(dest_out);
        }
    }
    let null = File::open("/dev/null")?;
    unsafe {
        if dup2(null.as_raw_fd(), 0) < 0 {
            return Err(io::Error::last_os_error());
        }
    }
    drop(null);
    Ok((File::from(proto_in), File::from(proto_out)))
}

#[cfg(windows)]
fn windows_steal() -> io::Result<(File, File)> {
    use std::os::windows::io::{FromRawHandle, RawHandle};
    use windows_sys::Win32::Foundation::{
        CloseHandle, DuplicateHandle, SetHandleInformation, GENERIC_READ, HANDLE,
        HANDLE_FLAG_INHERIT, INVALID_HANDLE_VALUE,
    };
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
    };
    use windows_sys::Win32::System::Console::{
        GetStdHandle, SetStdHandle, STD_ERROR_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE,
    };
    use windows_sys::Win32::System::Threading::GetCurrentProcess;

    const DUPLICATE_SAME_ACCESS: u32 = 0x0000_0002;

    unsafe fn dup_handle(src: HANDLE) -> io::Result<HANDLE> {
        if src.is_null() || src == INVALID_HANDLE_VALUE {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                "standard handle missing",
            ));
        }
        let proc = GetCurrentProcess();
        let mut dst: HANDLE = std::ptr::null_mut();
        let ok = DuplicateHandle(proc, src, proc, &mut dst, 0, 0, DUPLICATE_SAME_ACCESS);
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        let _ = SetHandleInformation(dst, HANDLE_FLAG_INHERIT, 0);
        Ok(dst)
    }

    unsafe {
        let stdin = GetStdHandle(STD_INPUT_HANDLE);
        let stdout = GetStdHandle(STD_OUTPUT_HANDLE);
        let stderr = GetStdHandle(STD_ERROR_HANDLE);
        let proto_in = dup_handle(stdin)?;
        let proto_out = dup_handle(stdout)?;

        let user_out = std::env::var(USER_STDOUT_HANDLE_ENV)
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .map(|n| n as HANDLE);
        std::env::remove_var(USER_STDOUT_HANDLE_ENV);
        let dest_out = user_out.unwrap_or(stderr);
        if SetStdHandle(STD_OUTPUT_HANDLE, dest_out) == 0 {
            return Err(io::Error::last_os_error());
        }
        let _ = crt_dup_handle_to_fd(dest_out, 1);
        let nul_name: Vec<u16> = "\\\\.\\NUL\0".encode_utf16().collect();
        let nul = CreateFileW(
            nul_name.as_ptr(),
            GENERIC_READ,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            std::ptr::null(),
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL,
            std::ptr::null_mut(),
        );
        if nul == INVALID_HANDLE_VALUE {
            return Err(io::Error::last_os_error());
        }
        if SetStdHandle(STD_INPUT_HANDLE, nul) == 0 {
            let _ = CloseHandle(nul);
            return Err(io::Error::last_os_error());
        }
        let _ = crt_dup_handle_to_fd(nul, 0);

        Ok((
            File::from_raw_handle(proto_in as RawHandle),
            File::from_raw_handle(proto_out as RawHandle),
        ))
    }
}

/// Point a CRT fd at `handle` so Regina `SAY` / `printf` match `SetStdHandle`.
#[cfg(windows)]
unsafe fn crt_dup_handle_to_fd(
    handle: windows_sys::Win32::Foundation::HANDLE,
    crt_fd: i32,
) -> io::Result<()> {
    use windows_sys::Win32::Foundation::{CloseHandle, DuplicateHandle, HANDLE};
    use windows_sys::Win32::System::Threading::GetCurrentProcess;
    const DUPLICATE_SAME_ACCESS: u32 = 0x0000_0002;
    const O_BINARY: i32 = 0x8000;
    extern "C" {
        fn _dup2(fd1: i32, fd2: i32) -> i32;
        fn _open_osfhandle(osfhandle: isize, flags: i32) -> i32;
        fn _close(fd: i32) -> i32;
    }
    let proc = GetCurrentProcess();
    let mut duped: HANDLE = std::ptr::null_mut();
    if DuplicateHandle(proc, handle, proc, &mut duped, 0, 0, DUPLICATE_SAME_ACCESS) == 0 {
        return Err(io::Error::last_os_error());
    }
    let fd = _open_osfhandle(duped as isize, O_BINARY);
    if fd < 0 {
        let _ = CloseHandle(duped);
        return Err(io::Error::last_os_error());
    }
    if _dup2(fd, crt_fd) < 0 {
        let _ = _close(fd);
        return Err(io::Error::last_os_error());
    }
    if fd != crt_fd {
        let _ = _close(fd);
    }
    Ok(())
}
