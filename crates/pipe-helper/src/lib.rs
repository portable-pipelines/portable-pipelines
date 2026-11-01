//! Shared framing, child spawn, and `StageCtx` drive loop for out-of-process
//! language stages (Python today, Regina REXX via a `pipe` helper).

pub mod child;
pub mod drive;
pub mod protocol;

pub use child::{spawn_piped, ChildIo, SpawnOpts};
pub use drive::drive;
pub use protocol::{
    decode_hello, decode_ok, decode_short, decode_stream_payload, encode_hello,
    encode_hello_with_meta, encode_ok, encode_ok_stream, encode_short, encode_stream_payload,
    read_frame, write_frame, Frame, HelloMeta, ShortSide, EOF, ERR, EXIT, FAIL, HELLO, HELLO_OK,
    OK, OUTPUT, PEEKTO, READTO, RECORD, SHORT, SHORT_SEVER_OUTPUT,
};

/// Hidden argv[1] that makes this `pipe` process host Regina instead of
/// running a specification. Not listed in `--help`.
pub const REGINA_HELPER_ARG: &str = "--internal-regina-helper";
