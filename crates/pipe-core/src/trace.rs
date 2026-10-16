//! Lightweight PIPE TRACE logging (CMS-inspired).
//!
//! Off by default. When enabled, the dispatcher and `StageCtx` emit one line
//! per interesting event to stderr (or a capture buffer for tests).

use std::cell::RefCell;
use std::fmt::Write as _;
use std::rc::Rc;

use crate::record::Record;

/// Where TRACE lines go.
#[derive(Clone)]
pub enum TraceSink {
    /// Write to stderr with a `PIPE TRACE:` prefix.
    Stderr,
    /// Append lines (without prefix) into a shared buffer.
    Capture(Rc<RefCell<Vec<String>>>),
}

/// TRACE configuration for one pipeline run.
#[derive(Clone)]
pub struct TraceConfig {
    enabled: bool,
    /// Max characters of record preview (UTF-8 lossy); 0 disables preview text.
    preview_chars: usize,
    sink: TraceSink,
}

impl Default for TraceConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            preview_chars: 40,
            sink: TraceSink::Stderr,
        }
    }
}

impl TraceConfig {
    pub fn off() -> Self {
        Self::default()
    }

    pub fn stderr() -> Self {
        Self {
            enabled: true,
            preview_chars: 40,
            sink: TraceSink::Stderr,
        }
    }

    pub fn capture(buf: Rc<RefCell<Vec<String>>>) -> Self {
        Self {
            enabled: true,
            preview_chars: 40,
            sink: TraceSink::Capture(buf),
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn with_preview_chars(mut self, n: usize) -> Self {
        self.preview_chars = n;
        self
    }

    pub fn emit(&self, message: impl AsRef<str>) {
        if !self.enabled {
            return;
        }
        let msg = message.as_ref();
        match &self.sink {
            TraceSink::Stderr => eprintln!("PIPE TRACE: {msg}"),
            TraceSink::Capture(buf) => buf.borrow_mut().push(msg.to_string()),
        }
    }

    pub fn preview(&self, record: &Record) -> String {
        if self.preview_chars == 0 {
            return String::new();
        }
        let s = record.to_string_lossy();
        let mut out = String::new();
        let mut chars = s.chars();
        for _ in 0..self.preview_chars {
            match chars.next() {
                Some(c) => {
                    if c == '\n' {
                        out.push_str("\\n");
                    } else if c == '\r' {
                        out.push_str("\\r");
                    } else if c == '"' {
                        out.push_str("\\\"");
                    } else {
                        out.push(c);
                    }
                }
                None => break,
            }
        }
        if chars.next().is_some() {
            out.push('…');
        }
        let mut quoted = String::new();
        let _ = write!(quoted, " \"{out}\"");
        quoted
    }
}
