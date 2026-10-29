//! Portable Pipelines specification scanner and parser.
//!
//! Supports stage separators `|` (self-escaping `||`), labels (`L:`),
//! `(ENDCHAR x)` / `(ESCAPE x)` / `(SEPARATOR x)` options, delimited strings,
//! and `*:`, `*.input.n:`, `*.output.n:` connectors for CALLPIPE/ADDPIPE.

mod operands;
mod scanner;
mod spec;

pub use operands::{
    decode_dstring, disk_operands, file_operands, is_dstring, operands_for_stage,
    scan_dstring_prefix, scan_operands, stage_args, word_operands,
};
pub use scanner::{
    scan, scan_spanned, scan_spanned_with, scan_with, ConnectorKind, ConnectorToken, ScanConfig,
    SpannedToken, Token,
};
pub use spec::{
    parse_pipeline, parse_with_options, ParseError, ParsedPipeline, ParsedStage, PipeOptions,
    StageArg,
};
