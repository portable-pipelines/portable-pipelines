//! CMS Pipelines specification scanner and parser.
//!
//! Supports stage separators `|`, labels (`L:`), `(ENDCHAR x)` options,
//! delimited strings, and `*:` connectors for CALLPIPE/ADDPIPE.

mod scanner;
mod spec;

pub use scanner::{scan, Token};
pub use spec::{
    parse_pipeline, parse_with_options, ConnectorKind, ParsedPipeline, ParsedStage, PipeOptions,
    StageArg, ParseError,
};
