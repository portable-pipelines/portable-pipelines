use std::fmt;

pub type PipeResult<T> = Result<T, PipeError>;

/// Where a stage lives in a pipeline set (Author's Edition 1.1.11 messages 1, 3, 4).
///
/// Pipeline and stage numbers are 1-based, matching CMS. `running` is the stage
/// specification shown in message 1 (`... Running "…"`). `name` is the PIPE
/// `NAME` option; when set, message 4 is issued instead of message 3.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StageOrigin {
    pub pipeline: u32,
    pub stage: u32,
    pub running: String,
    pub name: Option<String>,
}

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

    /// Attach CMS messages 3/4 and 1 when this error is attributable to a stage.
    pub fn with_origin(self, origin: StageOrigin) -> Self {
        match self {
            Self::Severed | Self::Aborted => self,
            Self::Message(m) => Self::Message(append_stage_context(&m, &origin)),
            Self::Stage(m) => Self::Stage(append_stage_context(&m, &origin)),
            Self::Parse(m) => Self::Parse(append_stage_context(&m, &origin)),
            Self::Io(m) => Self::Io(append_stage_context(&m, &origin)),
        }
    }

    /// Attach message 2 (`... Processing "callpipe …"`) for pipeline commands.
    pub fn with_processing(self, command: impl AsRef<str>) -> Self {
        match self {
            Self::Severed | Self::Aborted => self,
            Self::Message(m) => Self::Message(insert_processing_context(&m, command.as_ref())),
            Self::Stage(m) => Self::Stage(insert_processing_context(&m, command.as_ref())),
            Self::Parse(m) => Self::Parse(insert_processing_context(&m, command.as_ref())),
            Self::Io(m) => Self::Io(insert_processing_context(&m, command.as_ref())),
        }
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

/// True when `text` already includes CMS stage-context lines.
pub fn has_stage_context(text: &str) -> bool {
    text.contains("\n... Issued from stage ") || text.starts_with("... Issued from stage ")
}

/// Author's Edition message 3 or 4, then message 1 (default MSGLEVEL bits 4 and 1).
fn append_stage_context(msg: &str, origin: &StageOrigin) -> String {
    if has_stage_context(msg) {
        return msg.to_string();
    }
    let mut out = msg.to_string();
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    match &origin.name {
        Some(name) => {
            // Message 4: NAME option is set.
            out.push_str(&format!(
                "... Issued from stage {} of pipeline {} name \"{}\"\n",
                origin.stage, origin.pipeline, name
            ));
        }
        None => {
            out.push_str(&format!(
                "... Issued from stage {} of pipeline {}\n",
                origin.stage, origin.pipeline
            ));
        }
    }
    out.push_str("... Running \"");
    out.push_str(&origin.running);
    out.push('"');
    out
}

/// Author's Edition message 2, inserted after the primary line (default MSGLEVEL bit 2).
fn insert_processing_context(msg: &str, command: &str) -> String {
    if msg.contains("\n... Processing \"") || msg.starts_with("... Processing \"") {
        return msg.to_string();
    }
    let line = format!("... Processing \"{command}\"");
    match msg.split_once('\n') {
        Some((first, rest)) => format!("{first}\n{line}\n{rest}"),
        None => {
            if msg.is_empty() {
                line
            } else {
                format!("{msg}\n{line}")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn origin() -> StageOrigin {
        StageOrigin {
            pipeline: 1,
            stage: 3,
            running: "PICK 1.1 == /B/".into(),
            name: None,
        }
    }

    #[test]
    fn message_3_and_1_follow_the_primary() {
        let err = PipeError::stage("PICK: secondary input stream must not be connected")
            .with_origin(origin());
        assert_eq!(
            err.to_string(),
            "PICK: secondary input stream must not be connected\n\
             ... Issued from stage 3 of pipeline 1\n\
             ... Running \"PICK 1.1 == /B/\""
        );
    }

    #[test]
    fn message_4_when_name_is_set() {
        let mut o = origin();
        o.pipeline = 1;
        o.stage = 1;
        o.running = "unknown".into();
        o.name = Some("test".into());
        let err = PipeError::parse("unknown stage: UNKNOWN").with_origin(o);
        assert_eq!(
            err.to_string(),
            "unknown stage: UNKNOWN\n\
             ... Issued from stage 1 of pipeline 1 name \"test\"\n\
             ... Running \"unknown\""
        );
    }

    #[test]
    fn message_2_sits_between_primary_and_location() {
        let err = PipeError::parse("unknown stage: FOO")
            .with_origin(StageOrigin {
                pipeline: 1,
                stage: 2,
                running: "foo".into(),
                name: None,
            })
            .with_processing("callpipe literal x | foo");
        assert_eq!(
            err.to_string(),
            "unknown stage: FOO\n\
             ... Processing \"callpipe literal x | foo\"\n\
             ... Issued from stage 2 of pipeline 1\n\
             ... Running \"foo\""
        );
    }

    #[test]
    fn context_is_not_duplicated() {
        let err = PipeError::stage("boom")
            .with_origin(origin())
            .with_origin(origin())
            .with_processing("callpipe x")
            .with_processing("callpipe x");
        let s = err.to_string();
        assert_eq!(s.matches("Issued from stage").count(), 1);
        assert_eq!(s.matches("Processing").count(), 1);
        assert_eq!(s.matches("Running").count(), 1);
    }
}
