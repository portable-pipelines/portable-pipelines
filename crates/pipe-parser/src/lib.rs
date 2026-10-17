//! CMS Pipelines specification scanner and parser.
//!
//! Supports stage separators `|`, labels (`L:`), `(ENDCHAR x)` options,
//! delimited strings, and `*:` connectors for CALLPIPE/ADDPIPE.

mod operands;
mod scanner;
mod spec;

pub use operands::{disk_operands, operands_for_stage, scan_operands, stage_args, word_operands};
pub use scanner::{scan, scan_spanned, SpannedToken, Token};
pub use spec::{
    parse_pipeline, parse_with_options, ConnectorKind, ParseError, ParsedPipeline, ParsedStage,
    PipeOptions, StageArg,
};
