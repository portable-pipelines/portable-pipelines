//! Shared framing, child spawn, and `StageCtx` drive loop for out-of-process
//! language stages (Python today, Regina REXX via a `pipe` helper).

pub mod child;
pub mod drive;
pub mod protocol;

pub use child::{spawn_piped, ChildIo, SpawnOpts};
pub use drive::{drive, drive_with, with_varpool_child, DriveHooks};
pub use protocol::{
    decode_addstream, decode_callpipe, decode_hello, decode_ok, decode_short, decode_shv_reply,
    decode_shv_request, decode_stream_payload, decode_streamstate, decode_wait_any,
    encode_addstream, encode_callpipe, encode_commit, encode_hello, encode_hello_with_meta,
    encode_ok, encode_ok_stream, encode_short, encode_shv_err, encode_shv_ok, encode_shv_ok_list,
    encode_shv_ok_value, encode_shv_request, encode_stream_payload, encode_streamstate,
    encode_wait_any, read_frame, write_frame, Frame, HelloMeta, ShortSide, ShvReply, ShvRequest,
    ADDSTREAM, CALLPIPE, COMMIT, EOF, ERR, EXIT, FAIL, HELLO, HELLO_OK, OK, OUTPUT, PEEKTO, READTO,
    RECORD, SHORT, SHORT_SEVER_OUTPUT, SHV, SHV_DROP, SHV_GET, SHV_LIST, SHV_REPLY, SHV_SET,
    STREAMSTATE, WAIT_ANY,
};

/// Hidden argv[1] that makes this `pipe` process host Regina instead of
/// running a specification. Not listed in `--help`.
pub const REGINA_HELPER_ARG: &str = "--internal-regina-helper";
