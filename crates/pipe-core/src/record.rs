use std::fmt;

/// Stream number within a stage (0 = primary, 1 = secondary, …).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct StreamId(pub u32);

impl StreamId {
    pub const PRIMARY: StreamId = StreamId(0);
    pub const SECONDARY: StreamId = StreamId(1);
    pub const TERTIARY: StreamId = StreamId(2);
}

impl fmt::Display for StreamId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// One pipeline record: zero or more bytes (CMS Pipelines unit of transfer).
#[derive(Clone, PartialEq, Eq)]
pub struct Record {
    data: Vec<u8>,
}

impl Record {
    pub fn new(data: impl Into<Vec<u8>>) -> Self {
        Self { data: data.into() }
    }

    pub fn empty() -> Self {
        Self { data: Vec::new() }
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.data
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.data
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    pub fn from_utf8_lossy_string(s: impl AsRef<str>) -> Self {
        Self::new(s.as_ref().as_bytes().to_vec())
    }

    pub fn to_string_lossy(&self) -> String {
        String::from_utf8_lossy(&self.data).into_owned()
    }
}

impl fmt::Debug for Record {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match std::str::from_utf8(&self.data) {
            Ok(s) => write!(f, "Record({s:?})"),
            Err(_) => write!(f, "Record({} bytes)", self.data.len()),
        }
    }
}

impl From<Vec<u8>> for Record {
    fn from(data: Vec<u8>) -> Self {
        Self { data }
    }
}

impl From<&[u8]> for Record {
    fn from(data: &[u8]) -> Self {
        Self {
            data: data.to_vec(),
        }
    }
}

impl From<String> for Record {
    fn from(s: String) -> Self {
        Self {
            data: s.into_bytes(),
        }
    }
}

impl From<&str> for Record {
    fn from(s: &str) -> Self {
        Self {
            data: s.as_bytes().to_vec(),
        }
    }
}
