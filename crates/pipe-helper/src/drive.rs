//! Map child protocol requests onto `StageCtx`.

use std::cell::RefCell;
use std::rc::Rc;

use pipe_core::{PipeError, PipeResult, Record, StageCtx, StreamId};

use crate::child::{protocol_io_error, ChildIo};
use crate::protocol::{
    decode_addstream, decode_callpipe, decode_i32_le, decode_short, decode_stream_payload,
    decode_streamstate, decode_wait_any, encode_err, encode_ok_stream, encode_stream_payload,
    Frame, ShortSide, ADDPIPE, ADDSTREAM, CALLPIPE, COMMIT, EOF as EOF_TYPE, ERR, EXIT, FAIL,
    HELLO, HELLO_OK, OK, OUTPUT, PEEKTO, READTO, RECORD, SHORT, STREAMSTATE, WAIT_ANY,
};

type SharedChild = Rc<RefCell<ChildIo>>;

thread_local! {
    /// Helpers whose CALLPIPE is in flight. STEM/VAR in the parent uses the
    /// innermost session to reach that helper's Regina pool.
    static VARPOOL_IO: RefCell<Vec<SharedChild>> = RefCell::new(Vec::new());
}

/// Run `f` with the helper I/O for the innermost in-flight CALLPIPE.
pub fn with_varpool_child<T>(f: impl FnOnce(&mut ChildIo) -> T) -> Option<T> {
    let child = VARPOOL_IO.with(|stack| stack.borrow().last().cloned())?;
    let result = f(&mut child.borrow_mut());
    Some(result)
}

struct VarpoolGuard;

impl Drop for VarpoolGuard {
    fn drop(&mut self) {
        VARPOOL_IO.with(|stack| {
            stack.borrow_mut().pop();
        });
    }
}

fn push_varpool(child: &SharedChild) -> VarpoolGuard {
    VARPOOL_IO.with(|stack| stack.borrow_mut().push(Rc::clone(child)));
    VarpoolGuard
}

fn unwrap_child(child: SharedChild) -> ChildIo {
    VARPOOL_IO.with(|stack| stack.borrow_mut().clear());
    match Rc::try_unwrap(child) {
        Ok(cell) => cell.into_inner(),
        Err(rc) => Rc::try_unwrap(rc)
            .unwrap_or_else(|_| {
                unreachable!("helper I/O still shared after the varpool stack was cleared")
            })
            .into_inner(),
    }
}

/// Extra verbs the parent can honour. Python leaves these unset.
pub struct DriveHooks<'a> {
    pub callpipe: Option<&'a mut dyn FnMut(&mut StageCtx, &str) -> PipeResult<()>>,
    pub addpipe: Option<&'a mut dyn FnMut(&mut StageCtx, &str) -> PipeResult<()>>,
}

impl Default for DriveHooks<'_> {
    fn default() -> Self {
        Self {
            callpipe: None,
            addpipe: None,
        }
    }
}

pub fn drive(ctx: &mut StageCtx, child: ChildIo, kind: &str, hello: &[u8]) -> PipeResult<()> {
    drive_with(ctx, child, kind, hello, DriveHooks::default())
}

pub fn drive_with(
    ctx: &mut StageCtx,
    child: ChildIo,
    kind: &str,
    hello: &[u8],
    mut hooks: DriveHooks<'_>,
) -> PipeResult<()> {
    let child = Rc::new(RefCell::new(child));
    let result = drive_loop(ctx, &child, kind, hello, &mut hooks);
    let child = unwrap_child(child);
    match result {
        Ok(()) => {
            let _ = child.wait_exit();
            Ok(())
        }
        Err(e) => {
            child.terminate();
            Err(e)
        }
    }
}

fn drive_loop(
    ctx: &mut StageCtx,
    child: &SharedChild,
    kind: &str,
    hello: &[u8],
    hooks: &mut DriveHooks<'_>,
) -> PipeResult<()> {
    {
        let mut io = child.borrow_mut();
        io.write_frame(HELLO, hello)
            .map_err(|e| protocol_io_error(&mut io, e, kind))?;
    }

    let hello_reply = next_frame(ctx, child, kind)?;
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
        let frame = next_frame(ctx, child, kind)?;
        match frame.typ {
            READTO => handle_readto(ctx, child, kind, &frame.payload)?,
            PEEKTO => handle_peekto(ctx, child, kind, &frame.payload)?,
            OUTPUT => handle_output(ctx, child, kind, &frame.payload)?,
            SHORT => handle_short(ctx, child, kind, &frame.payload)?,
            CALLPIPE => handle_callpipe(ctx, child, kind, &frame.payload, hooks)?,
            ADDPIPE => handle_addpipe(ctx, child, kind, &frame.payload, hooks)?,
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
                let _ = child.borrow_mut().write_frame(ERR, &encode_err(8, &msg));
                return Err(PipeError::stage(msg));
            }
        }
    }
}

/// Park on [`StageCtx::wait_helper`] when the child has no frame yet so Stop
/// can abort_and_drain instead of blocking inside `resume_stage` on a helper
/// that is computing, sleeping, or otherwise not writing.
fn next_frame(ctx: &mut StageCtx, child: &SharedChild, kind: &str) -> PipeResult<Frame> {
    loop {
        {
            let mut io = child.borrow_mut();
            match io.try_read_frame() {
                Ok(Some(frame)) => return Ok(frame),
                Ok(None) => {
                    if io.has_exited() {
                        match io.try_read_after_exit() {
                            Ok(Some(frame)) => return Ok(frame),
                            Ok(None) => {
                                return Err(protocol_io_error(
                                    &mut io,
                                    std::io::Error::new(
                                        std::io::ErrorKind::UnexpectedEof,
                                        "helper pipe closed",
                                    ),
                                    kind,
                                ));
                            }
                            Err(e) => return Err(protocol_io_error(&mut io, e, kind)),
                        }
                    }
                }
                Err(e) => return Err(protocol_io_error(&mut io, e, kind)),
            }
        }
        ctx.wait_helper()?;
    }
}

fn write_frame(child: &SharedChild, typ: u8, payload: &[u8], kind: &str) -> PipeResult<()> {
    let mut io = child.borrow_mut();
    io.write_frame(typ, payload)
        .map_err(|e| protocol_io_error(&mut io, e, kind))
}

fn handle_readto(
    ctx: &mut StageCtx,
    child: &SharedChild,
    kind: &str,
    payload: &[u8],
) -> PipeResult<()> {
    let (stream, _) = decode_stream_payload(payload);
    ctx.select_input(StreamId(stream));
    match ctx.read() {
        Ok(Some(r)) => write_frame(
            child,
            RECORD,
            &encode_stream_payload(stream, r.as_bytes()),
            kind,
        ),
        Ok(None) | Err(PipeError::Severed) => {
            write_frame(child, EOF_TYPE, &encode_stream_payload(stream, &[]), kind)
        }
        Err(PipeError::Aborted) => Err(PipeError::Aborted),
        Err(e) => Err(e),
    }
}

fn handle_peekto(
    ctx: &mut StageCtx,
    child: &SharedChild,
    kind: &str,
    payload: &[u8],
) -> PipeResult<()> {
    let (stream, _) = decode_stream_payload(payload);
    ctx.select_input(StreamId(stream));
    match peekto_record(ctx) {
        Ok(Some(r)) => write_frame(
            child,
            RECORD,
            &encode_stream_payload(stream, r.as_bytes()),
            kind,
        ),
        Ok(None) | Err(PipeError::Severed) => {
            write_frame(child, EOF_TYPE, &encode_stream_payload(stream, &[]), kind)
        }
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
    child: &SharedChild,
    kind: &str,
    payload: &[u8],
) -> PipeResult<()> {
    let (stream, data) = decode_stream_payload(payload);
    ctx.select_output(StreamId(stream));
    match ctx.write(Record::from(data)) {
        Ok(()) => write_frame(child, OK, &encode_ok_stream(stream, 0), kind),
        Err(PipeError::Severed) => write_frame(child, OK, &encode_ok_stream(stream, 12), kind),
        Err(PipeError::Aborted) => Err(PipeError::Aborted),
        Err(e) => {
            let msg = e.to_string();
            let _ = child.borrow_mut().write_frame(ERR, &encode_err(8, &msg));
            Err(e)
        }
    }
}

fn handle_short(
    ctx: &mut StageCtx,
    child: &SharedChild,
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
    write_frame(child, OK, &encode_ok_stream(stream, 0), kind)
}

fn handle_callpipe(
    ctx: &mut StageCtx,
    child: &SharedChild,
    kind: &str,
    payload: &[u8],
    hooks: &mut DriveHooks<'_>,
) -> PipeResult<()> {
    let spec = decode_callpipe(payload)
        .map_err(|e| PipeError::stage(format!("{kind} CALLPIPE payload: {e}")))?;
    let Some(hook) = hooks.callpipe.as_mut() else {
        let msg = format!("{kind} CALLPIPE is not supported on this path");
        let _ = child.borrow_mut().write_frame(ERR, &encode_err(8, &msg));
        return Err(PipeError::stage(msg));
    };
    let _varpool = push_varpool(child);
    match hook(ctx, &spec) {
        Ok(()) => write_frame(child, OK, &encode_ok_stream(0, 0), kind),
        Err(PipeError::Aborted) => Err(PipeError::Aborted),
        Err(e) => {
            let msg = e.to_string();
            let _ = child.borrow_mut().write_frame(ERR, &encode_err(8, &msg));
            Err(e)
        }
    }
}

fn handle_addpipe(
    ctx: &mut StageCtx,
    child: &SharedChild,
    kind: &str,
    payload: &[u8],
    hooks: &mut DriveHooks<'_>,
) -> PipeResult<()> {
    let spec = decode_callpipe(payload)
        .map_err(|e| PipeError::stage(format!("{kind} ADDPIPE payload: {e}")))?;
    let Some(hook) = hooks.addpipe.as_mut() else {
        let msg = format!("{kind} ADDPIPE is not supported on this path");
        let _ = child.borrow_mut().write_frame(ERR, &encode_err(8, &msg));
        return Err(PipeError::stage(msg));
    };
    match hook(ctx, &spec) {
        Ok(()) => write_frame(child, OK, &encode_ok_stream(0, 0), kind),
        Err(PipeError::Aborted) => Err(PipeError::Aborted),
        Err(e) => {
            let msg = e.to_string();
            let _ = child.borrow_mut().write_frame(ERR, &encode_err(8, &msg));
            Err(e)
        }
    }
}

fn handle_commit(
    ctx: &mut StageCtx,
    child: &SharedChild,
    kind: &str,
    payload: &[u8],
) -> PipeResult<()> {
    let level = decode_i32_le(payload)
        .map_err(|e| PipeError::stage(format!("{kind} COMMIT payload: {e}")))?;
    match ctx.commit(level) {
        Ok(()) => write_frame(child, OK, &encode_ok_stream(0, 0), kind),
        Err(PipeError::Aborted) => Err(PipeError::Aborted),
        Err(e) => {
            let msg = e.to_string();
            let _ = child.borrow_mut().write_frame(ERR, &encode_err(8, &msg));
            Err(e)
        }
    }
}

fn handle_addstream(
    ctx: &mut StageCtx,
    child: &SharedChild,
    kind: &str,
    payload: &[u8],
) -> PipeResult<()> {
    let (input, output, number) = decode_addstream(payload)
        .map_err(|e| PipeError::stage(format!("{kind} ADDSTREAM payload: {e}")))?;
    let n = ctx.add_stream(input, output, number);
    write_frame(child, OK, &encode_ok_stream(0, n as i32), kind)
}

fn handle_streamstate(
    ctx: &mut StageCtx,
    child: &SharedChild,
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
    write_frame(child, OK, &encode_ok_stream(stream_n, rc), kind)
}

fn handle_wait_any(
    ctx: &mut StageCtx,
    child: &SharedChild,
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
            let _ = child.borrow_mut().write_frame(ERR, &encode_err(8, &msg));
            return Err(e);
        }
    }
    if let Some(&s) = streams.iter().find(|&&s| ctx.input_record_ready(s)) {
        return write_frame(child, OK, &encode_ok_stream(s.0, 0), kind);
    }
    if streams
        .iter()
        .all(|&s| ctx.input_at_eof(s) || !ctx.input_connected(s))
    {
        return write_frame(child, EOF_TYPE, &encode_stream_payload(0, &[]), kind);
    }
    write_frame(child, OK, &encode_ok_stream(0, 0), kind)
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
