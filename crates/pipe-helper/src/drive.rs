//! Map child protocol requests onto `StageCtx`.

use pipe_core::{PipeError, PipeResult, Record, StageCtx, StreamId};

use crate::child::{protocol_io_error, ChildIo};
use crate::protocol::{
    decode_i32_le, decode_short, decode_stream_payload, encode_err, encode_ok_stream,
    encode_stream_payload, ShortSide, EOF as EOF_TYPE, ERR, EXIT, FAIL, HELLO, HELLO_OK, OK,
    OUTPUT, PEEKTO, READTO, RECORD, SHORT,
};

pub fn drive(ctx: &mut StageCtx, child: &mut ChildIo, kind: &str, hello: &[u8]) -> PipeResult<()> {
    child
        .write_frame(HELLO, hello)
        .map_err(|e| protocol_io_error(child, e, kind))?;

    let hello_reply = child
        .read_frame()
        .map_err(|e| protocol_io_error(child, e, kind))?;
    match hello_reply.typ {
        HELLO_OK => {}
        FAIL => return Err(fail_from_child(&hello_reply.payload, kind)),
        other => {
            return Err(PipeError::stage(format!(
                "{kind} sent unexpected hello reply 0x{other:02x}"
            )));
        }
    }

    loop {
        let frame = match child.read_frame() {
            Ok(f) => f,
            Err(e) => return Err(protocol_io_error(child, e, kind)),
        };
        match frame.typ {
            READTO => handle_readto(ctx, child, kind, &frame.payload)?,
            PEEKTO => handle_peekto(ctx, child, kind, &frame.payload)?,
            OUTPUT => handle_output(ctx, child, kind, &frame.payload)?,
            SHORT => handle_short(ctx, child, kind, &frame.payload)?,
            EXIT => {
                let rc = decode_i32_le(&frame.payload)
                    .map_err(|e| PipeError::stage(format!("{kind} EXIT payload: {e}")))?;
                ctx.set_return_code(rc);
                return Ok(());
            }
            FAIL => return Err(fail_from_child(&frame.payload, kind)),
            other => {
                let msg = format!("unknown {kind} protocol type 0x{other:02x}");
                let _ = child.write_frame(ERR, &encode_err(8, &msg));
                return Err(PipeError::stage(msg));
            }
        }
    }
}

fn require_stream_zero(kind: &str, stream: u32) -> PipeResult<()> {
    if stream == 0 {
        Ok(())
    } else {
        Err(PipeError::stage(format!(
            "{kind} requested stream {stream}; only stream 0 is supported"
        )))
    }
}

fn handle_readto(
    ctx: &mut StageCtx,
    child: &mut ChildIo,
    kind: &str,
    payload: &[u8],
) -> PipeResult<()> {
    let (stream, _) = decode_stream_payload(payload);
    require_stream_zero(kind, stream)?;
    ctx.select_input(StreamId(stream));
    match ctx.read() {
        Ok(Some(r)) => child
            .write_frame(RECORD, &encode_stream_payload(stream, r.as_bytes()))
            .map_err(|e| protocol_io_error(child, e, kind)),
        Ok(None) => child
            .write_frame(EOF_TYPE, &encode_stream_payload(stream, &[]))
            .map_err(|e| protocol_io_error(child, e, kind)),
        Err(PipeError::Severed) => child
            .write_frame(EOF_TYPE, &encode_stream_payload(stream, &[]))
            .map_err(|e| protocol_io_error(child, e, kind)),
        Err(PipeError::Aborted) => Err(PipeError::Aborted),
        Err(e) => Err(e),
    }
}

fn handle_peekto(
    ctx: &mut StageCtx,
    child: &mut ChildIo,
    kind: &str,
    payload: &[u8],
) -> PipeResult<()> {
    let (stream, _) = decode_stream_payload(payload);
    require_stream_zero(kind, stream)?;
    ctx.select_input(StreamId(stream));
    match peekto_record(ctx) {
        Ok(Some(r)) => child
            .write_frame(RECORD, &encode_stream_payload(stream, r.as_bytes()))
            .map_err(|e| protocol_io_error(child, e, kind)),
        Ok(None) => child
            .write_frame(EOF_TYPE, &encode_stream_payload(stream, &[]))
            .map_err(|e| protocol_io_error(child, e, kind)),
        Err(PipeError::Severed) => child
            .write_frame(EOF_TYPE, &encode_stream_payload(stream, &[]))
            .map_err(|e| protocol_io_error(child, e, kind)),
        Err(PipeError::Aborted) => Err(PipeError::Aborted),
        Err(e) => Err(e),
    }
}

fn peekto_record(ctx: &mut StageCtx) -> PipeResult<Option<Record>> {
    let stream = ctx.selected_input();
    loop {
        ctx.wait_any_input(&[stream])?;
        if ctx.input_at_eof(stream) {
            return Ok(None);
        }
        if let Some(r) = ctx.peek() {
            return Ok(Some(r));
        }
        if !ctx.input_connected(stream) {
            return Ok(None);
        }
    }
}

fn handle_output(
    ctx: &mut StageCtx,
    child: &mut ChildIo,
    kind: &str,
    payload: &[u8],
) -> PipeResult<()> {
    let (stream, data) = decode_stream_payload(payload);
    require_stream_zero(kind, stream)?;
    ctx.select_output(StreamId(stream));
    match ctx.write(Record::from(data)) {
        Ok(()) => child
            .write_frame(OK, &encode_ok_stream(stream, 0))
            .map_err(|e| protocol_io_error(child, e, kind)),
        Err(PipeError::Severed) => child
            .write_frame(OK, &encode_ok_stream(stream, 12))
            .map_err(|e| protocol_io_error(child, e, kind)),
        Err(PipeError::Aborted) => Err(PipeError::Aborted),
        Err(e) => {
            let msg = e.to_string();
            let _ = child.write_frame(ERR, &encode_err(8, &msg));
            Err(e)
        }
    }
}

fn handle_short(
    ctx: &mut StageCtx,
    child: &mut ChildIo,
    kind: &str,
    payload: &[u8],
) -> PipeResult<()> {
    let (stream, side) = decode_short(payload);
    require_stream_zero(kind, stream)?;
    match side {
        ShortSide::Both => {
            ctx.select_input(StreamId(stream));
            ctx.select_output(StreamId(stream));
            ctx.short();
        }
        ShortSide::Input => {
            ctx.select_input(StreamId(stream));
            ctx.short_input();
        }
        ShortSide::Output => {
            ctx.select_output(StreamId(stream));
            ctx.short_output();
        }
        ShortSide::SeverOutput => {
            ctx.select_output(StreamId(stream));
            ctx.sever_output();
        }
    }
    child
        .write_frame(OK, &encode_ok_stream(stream, 0))
        .map_err(|e| protocol_io_error(child, e, kind))
}

fn fail_from_child(payload: &[u8], kind: &str) -> PipeError {
    let msg = String::from_utf8_lossy(payload);
    let msg = msg.trim();
    if msg.is_empty() {
        PipeError::stage(format!("{kind} failed"))
    } else {
        PipeError::stage(msg.to_string())
    }
}
