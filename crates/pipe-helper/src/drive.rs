//! Map child protocol requests onto `StageCtx`.

use pipe_core::{PipeError, PipeResult, Record, StageCtx, StreamId};

use crate::child::{protocol_io_error, ChildIo};
use crate::protocol::{
    decode_addstream, decode_callpipe, decode_i32_le, decode_short, decode_stream_payload,
    decode_streamstate, decode_wait_any, encode_err, encode_ok_stream, encode_stream_payload,
    ShortSide, ADDSTREAM, CALLPIPE, COMMIT, EOF as EOF_TYPE, ERR, EXIT, FAIL, HELLO, HELLO_OK, OK,
    OUTPUT, PEEKTO, READTO, RECORD, SHORT, STREAMSTATE, WAIT_ANY,
};

/// Extra verbs the parent can honour. Python leaves these unset.
pub struct DriveHooks<'a> {
    pub callpipe: Option<&'a mut dyn FnMut(&mut StageCtx, &str) -> PipeResult<()>>,
}

impl Default for DriveHooks<'_> {
    fn default() -> Self {
        Self { callpipe: None }
    }
}

pub fn drive(ctx: &mut StageCtx, child: &mut ChildIo, kind: &str, hello: &[u8]) -> PipeResult<()> {
    drive_with(ctx, child, kind, hello, DriveHooks::default())
}

pub fn drive_with(
    ctx: &mut StageCtx,
    child: &mut ChildIo,
    kind: &str,
    hello: &[u8],
    mut hooks: DriveHooks<'_>,
) -> PipeResult<()> {
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
            CALLPIPE => handle_callpipe(ctx, child, kind, &frame.payload, &mut hooks)?,
            COMMIT => handle_commit(ctx, child, kind, &frame.payload)?,
            ADDSTREAM => handle_addstream(ctx, child, kind, &frame.payload)?,
            STREAMSTATE => handle_streamstate(ctx, child, kind, &frame.payload)?,
            WAIT_ANY => handle_wait_any(ctx, child, kind, &frame.payload)?,
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

fn handle_readto(
    ctx: &mut StageCtx,
    child: &mut ChildIo,
    kind: &str,
    payload: &[u8],
) -> PipeResult<()> {
    let (stream, _) = decode_stream_payload(payload);
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

fn handle_callpipe(
    ctx: &mut StageCtx,
    child: &mut ChildIo,
    kind: &str,
    payload: &[u8],
    hooks: &mut DriveHooks<'_>,
) -> PipeResult<()> {
    let spec = decode_callpipe(payload)
        .map_err(|e| PipeError::stage(format!("{kind} CALLPIPE payload: {e}")))?;
    let Some(hook) = hooks.callpipe.as_mut() else {
        let msg = format!("{kind} CALLPIPE is not supported on this path");
        let _ = child.write_frame(ERR, &encode_err(8, &msg));
        return Err(PipeError::stage(msg));
    };
    match hook(ctx, &spec) {
        Ok(()) => child
            .write_frame(OK, &encode_ok_stream(0, 0))
            .map_err(|e| protocol_io_error(child, e, kind)),
        Err(PipeError::Aborted) => Err(PipeError::Aborted),
        Err(e) => {
            let msg = e.to_string();
            let _ = child.write_frame(ERR, &encode_err(8, &msg));
            Err(e)
        }
    }
}

fn handle_commit(
    ctx: &mut StageCtx,
    child: &mut ChildIo,
    kind: &str,
    payload: &[u8],
) -> PipeResult<()> {
    let level = decode_i32_le(payload)
        .map_err(|e| PipeError::stage(format!("{kind} COMMIT payload: {e}")))?;
    match ctx.commit(level) {
        Ok(()) => child
            .write_frame(OK, &encode_ok_stream(0, 0))
            .map_err(|e| protocol_io_error(child, e, kind)),
        Err(PipeError::Aborted) => Err(PipeError::Aborted),
        Err(e) => {
            let msg = e.to_string();
            let _ = child.write_frame(ERR, &encode_err(8, &msg));
            Err(e)
        }
    }
}

fn handle_addstream(
    ctx: &mut StageCtx,
    child: &mut ChildIo,
    kind: &str,
    payload: &[u8],
) -> PipeResult<()> {
    let (input, output, number) = decode_addstream(payload)
        .map_err(|e| PipeError::stage(format!("{kind} ADDSTREAM payload: {e}")))?;
    let n = ctx.add_stream(input, output, number);
    child
        .write_frame(OK, &encode_ok_stream(0, n as i32))
        .map_err(|e| protocol_io_error(child, e, kind))
}

fn handle_streamstate(
    ctx: &mut StageCtx,
    child: &mut ChildIo,
    kind: &str,
    payload: &[u8],
) -> PipeResult<()> {
    let (side_input, stream_n) = decode_streamstate(payload)
        .map_err(|e| PipeError::stage(format!("{kind} STREAMSTATE payload: {e}")))?;
    let stream = StreamId(stream_n);
    let rc = if side_input {
        if ctx.input_connected(stream) {
            0
        } else if ctx.input_defined(stream) {
            12
        } else {
            -4
        }
    } else if ctx.output_connected(stream) {
        0
    } else if ctx.output_defined(stream) {
        12
    } else {
        -4
    };
    child
        .write_frame(OK, &encode_ok_stream(stream_n, rc))
        .map_err(|e| protocol_io_error(child, e, kind))
}

fn handle_wait_any(
    ctx: &mut StageCtx,
    child: &mut ChildIo,
    kind: &str,
    payload: &[u8],
) -> PipeResult<()> {
    let ids = decode_wait_any(payload)
        .map_err(|e| PipeError::stage(format!("{kind} WAIT_ANY payload: {e}")))?;
    let streams: Vec<StreamId> = ids.iter().copied().map(StreamId).collect();
    match ctx.wait_any_input(&streams) {
        Ok(()) => {}
        Err(PipeError::Aborted) => return Err(PipeError::Aborted),
        Err(e) => {
            let msg = e.to_string();
            let _ = child.write_frame(ERR, &encode_err(8, &msg));
            return Err(e);
        }
    }
    if let Some(&s) = streams.iter().find(|&&s| ctx.input_record_ready(s)) {
        return child
            .write_frame(OK, &encode_ok_stream(s.0, 0))
            .map_err(|e| protocol_io_error(child, e, kind));
    }
    if streams
        .iter()
        .all(|&s| ctx.input_at_eof(s) || !ctx.input_connected(s))
    {
        return child
            .write_frame(EOF_TYPE, &encode_stream_payload(0, &[]))
            .map_err(|e| protocol_io_error(child, e, kind));
    }
    child
        .write_frame(OK, &encode_ok_stream(0, 0))
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
