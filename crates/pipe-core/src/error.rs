use std::fmt;

pub type PipeResult<T> = Result<T, PipeError>;

#[derive(Debug, Clone)]
pub enum PipeError {
    Message(String),
    Stage(String),
    Parse(String),
    Io(String),
    Severed,
    Aborted,
}

impl PipeError {
    pub fn msg(m: impl Into<String>) -> Self {
        Self::Message(m.into())
    }

    pub fn stage(m: impl Into<String>) -> Self {
        Self::Stage(m.into())
    }

    pub fn parse(m: impl Into<String>) -> Self {
        Self::Parse(m.into())
    }

    pub fn io(m: impl Into<String>) -> Self {
        Self::Io(m.into())
    }
}

impl fmt::Display for PipeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Message(m) | Self::Stage(m) | Self::Parse(m) | Self::Io(m) => write!(f, "{m}"),
            Self::Severed => write!(f, "stream severed"),
            Self::Aborted => write!(f, "pipeline aborted"),
        }
    }
}

impl std::error::Error for PipeError {}

impl From<std::io::Error> for PipeError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e.to_string())
    }
}
