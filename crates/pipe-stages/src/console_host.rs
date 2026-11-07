//! Optional host console replacement for the desktop window.
//!
//! When a hook is installed on the worker thread, CONSOLE writes go to the
//! hook instead of stdout, and CONSOLE reads return end-of-file immediately
//! so the window cannot block on stdin.

use std::cell::RefCell;
use std::sync::Arc;

use pipe_core::Record;

thread_local! {
    static HOOK: RefCell<Option<Arc<dyn ConsoleHost>>> = const { RefCell::new(None) };
}

/// Host console used by the GUI worker. Implementations must not block.
pub trait ConsoleHost: Send + Sync {
    fn write_record(&self, record: &Record);
    /// `None` means end-of-file. Must not wait for a human.
    fn read_record(&self) -> Option<Record>;
}

/// Run `f` with CONSOLE redirected to `host` on this thread.
pub fn with_console_host<R>(host: Arc<dyn ConsoleHost>, f: impl FnOnce() -> R) -> R {
    struct Guard(Option<Arc<dyn ConsoleHost>>);
    impl Drop for Guard {
        fn drop(&mut self) {
            HOOK.with(|slot| slot.replace(self.0.take()));
        }
    }
    let prev = HOOK.with(|slot| slot.replace(Some(host)));
    let _guard = Guard(prev);
    f()
}

pub(crate) fn hooked_write(record: &Record) -> bool {
    HOOK.with(|slot| {
        if let Some(host) = slot.borrow().as_ref() {
            host.write_record(record);
            true
        } else {
            false
        }
    })
}

pub(crate) fn hooked_read() -> Option<Option<Record>> {
    HOOK.with(|slot| slot.borrow().as_ref().map(|host| host.read_record()))
}
