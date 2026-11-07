use std::sync::Arc;

use pipe_core::Record;
use pipe_stages::{run_spec, with_console_host, ConsoleHost};

struct Capture {
    writes: std::sync::Mutex<Vec<String>>,
}

impl ConsoleHost for Capture {
    fn write_record(&self, record: &Record) {
        self.writes
            .lock()
            .unwrap()
            .push(String::from_utf8_lossy(record.as_bytes()).into_owned());
    }

    fn read_record(&self) -> Option<Record> {
        None
    }
}

#[test]
fn hooked_console_does_not_need_stdin_and_captures_writes() {
    let host = Arc::new(Capture {
        writes: std::sync::Mutex::new(Vec::new()),
    });
    with_console_host(host.clone(), || {
        run_spec("literal hello| console").unwrap();
        run_spec("console | hole").unwrap();
    });
    let got = host.writes.lock().unwrap().clone();
    assert_eq!(got, vec!["hello".to_string()]);
}
