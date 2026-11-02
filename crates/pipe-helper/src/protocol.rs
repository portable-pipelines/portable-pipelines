//! Little-endian framed protocol for out-of-process language stages.
//!
//! Layout: `u32le payload_len`, type byte, payload. I/O requests and
//! record/EOF/OK replies carry a `u32le` stream number (default 0).
//! Regina uses stream numbers, COMMIT, CALLPIPE, and related messages.
//! Python still sends stream 0 and the original I/O types only.

use std::io::{self, Read, Write};

pub const MAX_PAYLOAD: u32 = 16 * 1024 * 1024;

pub const HELLO: u8 = 0x01;
pub const READTO: u8 = 0x10;
pub const PEEKTO: u8 = 0x11;
pub const OUTPUT: u8 = 0x12;
pub const SHORT: u8 = 0x13;
pub const EXIT: u8 = 0x14;
pub const FAIL: u8 = 0x15;
pub const CALLPIPE: u8 = 0x16;
pub const COMMIT: u8 = 0x17;
pub const ADDSTREAM: u8 = 0x18;
pub const STREAMSTATE: u8 = 0x19;
pub const WAIT_ANY: u8 = 0x1A;
/// Parent to helper: Regina variable-pool op (GET / SET / LIST / DROP).
pub const SHV: u8 = 0x1B;
/// Helper to parent: ADDPIPE specification (same payload as CALLPIPE).
pub const ADDPIPE: u8 = 0x1C;

pub const HELLO_OK: u8 = 0x80;
pub const RECORD: u8 = 0x81;
pub const EOF: u8 = 0x82;
pub const OK: u8 = 0x83;
pub const ERR: u8 = 0x84;
/// Helper to parent: reply to [`SHV`].
pub const SHV_REPLY: u8 = 0x85;

pub const SHV_GET: u8 = 0;
pub const SHV_SET: u8 = 1;
pub const SHV_LIST: u8 = 2;
pub const SHV_DROP: u8 = 3;

/// SHORT payload side byte. Omitted (Python, 4-byte payload) means both.
pub const SHORT_BOTH: u8 = 0;
pub const SHORT_INPUT: u8 = 1;
pub const SHORT_OUTPUT: u8 = 2;
/// Wait for the in-flight record to be consumed, then sever (SEVER OUTPUT).
pub const SHORT_SEVER_OUTPUT: u8 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShortSide {
    Both,
    Input,
    Output,
    SeverOutput,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub typ: u8,
    pub payload: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HelloMeta {
    pub stage_num: u32,
    pub eof_report: u8,
    pub nocommit: bool,
    pub connected_in: Vec<u32>,
    pub connected_out: Vec<u32>,
}

impl Default for HelloMeta {
    fn default() -> Self {
        Self {
            stage_num: 1,
            eof_report: 0,
            nocommit: false,
            connected_in: vec![0],
            connected_out: vec![0],
        }
    }
}

pub fn write_frame<W: Write>(w: &mut W, typ: u8, payload: &[u8]) -> io::Result<()> {
    if payload.len() > MAX_PAYLOAD as usize {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "stage payload exceeds 16 MiB",
        ));
    }
    w.write_all(&(payload.len() as u32).to_le_bytes())?;
    w.write_all(&[typ])?;
    w.write_all(payload)?;
    w.flush()
}

pub fn read_frame<R: Read>(r: &mut R) -> io::Result<Frame> {
    let mut len_buf = [0u8; 4];
    r.read_exact(&mut len_buf)?;
    let len = u32::from_le_bytes(len_buf);
    if len > MAX_PAYLOAD {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "stage payload exceeds 16 MiB",
        ));
    }
    let mut typ = [0u8; 1];
    r.read_exact(&mut typ)?;
    let mut payload = vec![0u8; len as usize];
    if len > 0 {
        r.read_exact(&mut payload)?;
    }
    Ok(Frame {
        typ: typ[0],
        payload,
    })
}

pub fn encode_hello(path: &str, args: &[String]) -> Vec<u8> {
    encode_hello_with_meta(path, args, None)
}

pub fn encode_hello_with_meta(path: &str, args: &[String], meta: Option<&HelloMeta>) -> Vec<u8> {
    let mut p = Vec::new();
    p.extend_from_slice(&1u16.to_le_bytes());
    p.extend_from_slice(&0u32.to_le_bytes());
    let path_b = path.as_bytes();
    p.extend_from_slice(&(path_b.len() as u32).to_le_bytes());
    p.extend_from_slice(path_b);
    p.extend_from_slice(&(args.len() as u32).to_le_bytes());
    for a in args {
        let b = a.as_bytes();
        p.extend_from_slice(&(b.len() as u32).to_le_bytes());
        p.extend_from_slice(b);
    }
    if let Some(m) = meta {
        p.extend_from_slice(&m.stage_num.to_le_bytes());
        p.push(m.eof_report);
        p.push(if m.nocommit { 1 } else { 0 });
        p.extend_from_slice(&(m.connected_in.len() as u32).to_le_bytes());
        for s in &m.connected_in {
            p.extend_from_slice(&s.to_le_bytes());
        }
        p.extend_from_slice(&(m.connected_out.len() as u32).to_le_bytes());
        for s in &m.connected_out {
            p.extend_from_slice(&s.to_le_bytes());
        }
    }
    p
}

pub fn decode_hello(payload: &[u8]) -> io::Result<(String, Vec<String>, HelloMeta)> {
    if payload.len() < 10 {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "hello payload is truncated",
        ));
    }
    let version = u16::from_le_bytes(payload[0..2].try_into().unwrap());
    if version != 1 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("unsupported protocol version {version}"),
        ));
    }
    let path_len = u32::from_le_bytes(payload[6..10].try_into().unwrap()) as usize;
    let mut off = 10;
    if off + path_len + 4 > payload.len() {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "hello path is truncated",
        ));
    }
    let path = String::from_utf8_lossy(&payload[off..off + path_len]).into_owned();
    off += path_len;
    let argc = u32::from_le_bytes(payload[off..off + 4].try_into().unwrap()) as usize;
    off += 4;
    let mut args = Vec::with_capacity(argc);
    for _ in 0..argc {
        if off + 4 > payload.len() {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "hello arg length is truncated",
            ));
        }
        let n = u32::from_le_bytes(payload[off..off + 4].try_into().unwrap()) as usize;
        off += 4;
        if off + n > payload.len() {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "hello arg is truncated",
            ));
        }
        args.push(String::from_utf8_lossy(&payload[off..off + n]).into_owned());
        off += n;
    }
    let mut meta = HelloMeta::default();
    if off + 6 <= payload.len() {
        meta.stage_num = u32::from_le_bytes(payload[off..off + 4].try_into().unwrap());
        meta.eof_report = payload[off + 4];
        meta.nocommit = payload[off + 5] != 0;
        off += 6;
        if off + 4 <= payload.len() {
            let n_in = u32::from_le_bytes(payload[off..off + 4].try_into().unwrap()) as usize;
            off += 4;
            if off + n_in * 4 + 4 <= payload.len() {
                meta.connected_in.clear();
                for _ in 0..n_in {
                    meta.connected_in.push(u32::from_le_bytes(
                        payload[off..off + 4].try_into().unwrap(),
                    ));
                    off += 4;
                }
                let n_out = u32::from_le_bytes(payload[off..off + 4].try_into().unwrap()) as usize;
                off += 4;
                if off + n_out * 4 <= payload.len() {
                    meta.connected_out.clear();
                    for _ in 0..n_out {
                        meta.connected_out.push(u32::from_le_bytes(
                            payload[off..off + 4].try_into().unwrap(),
                        ));
                        off += 4;
                    }
                }
            }
        }
    }
    Ok((path, args, meta))
}

pub fn decode_i32_le(payload: &[u8]) -> io::Result<i32> {
    if payload.len() < 4 {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "expected i32 payload",
        ));
    }
    Ok(i32::from_le_bytes(payload[..4].try_into().unwrap()))
}

/// Stream number plus trailing bytes. Fewer than 4 bytes means stream 0.
pub fn decode_stream_payload(payload: &[u8]) -> (u32, &[u8]) {
    if payload.len() < 4 {
        return (0, payload);
    }
    let stream = u32::from_le_bytes(payload[..4].try_into().unwrap());
    (stream, &payload[4..])
}

pub fn encode_stream_payload(stream: u32, data: &[u8]) -> Vec<u8> {
    let mut p = stream.to_le_bytes().to_vec();
    p.extend_from_slice(data);
    p
}

pub fn encode_short(stream: u32, side: ShortSide) -> Vec<u8> {
    let mut p = stream.to_le_bytes().to_vec();
    let b = match side {
        ShortSide::Both => SHORT_BOTH,
        ShortSide::Input => SHORT_INPUT,
        ShortSide::Output => SHORT_OUTPUT,
        ShortSide::SeverOutput => SHORT_SEVER_OUTPUT,
    };
    p.push(b);
    p
}

pub fn decode_short(payload: &[u8]) -> (u32, ShortSide) {
    let (stream, rest) = decode_stream_payload(payload);
    let side = match rest.first().copied() {
        Some(SHORT_INPUT) => ShortSide::Input,
        Some(SHORT_OUTPUT) => ShortSide::Output,
        Some(SHORT_SEVER_OUTPUT) => ShortSide::SeverOutput,
        _ => ShortSide::Both,
    };
    (stream, side)
}

pub fn encode_ok(rc: i32) -> Vec<u8> {
    encode_ok_stream(0, rc)
}

pub fn encode_ok_stream(stream: u32, rc: i32) -> Vec<u8> {
    let mut p = stream.to_le_bytes().to_vec();
    p.extend_from_slice(&rc.to_le_bytes());
    p
}

/// RC from an OK payload. Accepts stream+rc (8 bytes) or a bare i32 (4 bytes).
pub fn decode_ok(payload: &[u8]) -> io::Result<i32> {
    if payload.len() >= 8 {
        return Ok(i32::from_le_bytes(payload[4..8].try_into().unwrap()));
    }
    decode_i32_le(payload)
}

pub fn encode_err(rc: i32, message: &str) -> Vec<u8> {
    let mut p = rc.to_le_bytes().to_vec();
    p.extend_from_slice(message.as_bytes());
    p
}

pub fn encode_exit(code: i32) -> Vec<u8> {
    code.to_le_bytes().to_vec()
}

pub fn encode_callpipe(spec: &str) -> Vec<u8> {
    spec.as_bytes().to_vec()
}

pub fn decode_callpipe(payload: &[u8]) -> io::Result<String> {
    String::from_utf8(payload.to_vec()).map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("CALLPIPE spec is not utf-8: {e}"),
        )
    })
}

pub fn encode_commit(level: i32) -> Vec<u8> {
    level.to_le_bytes().to_vec()
}

/// ADDSTREAM: `u8` sides (bit 0 input, bit 1 output), then optional `u32le` number.
pub fn encode_addstream(input: bool, output: bool, number: Option<u32>) -> Vec<u8> {
    let mut p = vec![0u8];
    if input {
        p[0] |= 1;
    }
    if output {
        p[0] |= 2;
    }
    if let Some(n) = number {
        p.extend_from_slice(&n.to_le_bytes());
    }
    p
}

pub fn decode_addstream(payload: &[u8]) -> io::Result<(bool, bool, Option<u32>)> {
    if payload.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "ADDSTREAM payload is truncated",
        ));
    }
    let input = payload[0] & 1 != 0;
    let output = payload[0] & 2 != 0;
    let number = if payload.len() >= 5 {
        Some(u32::from_le_bytes(payload[1..5].try_into().unwrap()))
    } else {
        None
    };
    Ok((input, output, number))
}

pub const SIDE_INPUT: u8 = 1;
pub const SIDE_OUTPUT: u8 = 2;

pub fn encode_streamstate(side_input: bool, stream: u32) -> Vec<u8> {
    let mut p = vec![if side_input { SIDE_INPUT } else { SIDE_OUTPUT }];
    p.extend_from_slice(&stream.to_le_bytes());
    p
}

pub fn decode_streamstate(payload: &[u8]) -> io::Result<(bool, u32)> {
    if payload.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "STREAMSTATE payload is truncated",
        ));
    }
    let side_input = payload[0] != SIDE_OUTPUT;
    let stream = if payload.len() >= 5 {
        u32::from_le_bytes(payload[1..5].try_into().unwrap())
    } else {
        0
    };
    Ok((side_input, stream))
}

pub fn encode_wait_any(streams: &[u32]) -> Vec<u8> {
    let mut p = (streams.len() as u32).to_le_bytes().to_vec();
    for s in streams {
        p.extend_from_slice(&s.to_le_bytes());
    }
    p
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShvRequest {
    pub op: u8,
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShvReply {
    pub ok: bool,
    pub message: String,
    pub value: String,
    pub list: Vec<(String, String)>,
}

fn push_len_bytes(p: &mut Vec<u8>, data: &[u8]) {
    p.extend_from_slice(&(data.len() as u32).to_le_bytes());
    p.extend_from_slice(data);
}

fn take_len_bytes(payload: &[u8], off: &mut usize) -> io::Result<Vec<u8>> {
    if *off + 4 > payload.len() {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "SHV length is truncated",
        ));
    }
    let n = u32::from_le_bytes(payload[*off..*off + 4].try_into().unwrap()) as usize;
    *off += 4;
    if *off + n > payload.len() {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "SHV bytes are truncated",
        ));
    }
    let out = payload[*off..*off + n].to_vec();
    *off += n;
    Ok(out)
}

pub fn encode_shv_request(op: u8, name: &str, value: &str) -> Vec<u8> {
    let mut p = vec![op];
    push_len_bytes(&mut p, name.as_bytes());
    push_len_bytes(&mut p, value.as_bytes());
    p
}

pub fn decode_shv_request(payload: &[u8]) -> io::Result<ShvRequest> {
    if payload.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "SHV request is truncated",
        ));
    }
    let op = payload[0];
    let mut off = 1;
    let name = String::from_utf8_lossy(&take_len_bytes(payload, &mut off)?).into_owned();
    let value = if off < payload.len() {
        String::from_utf8_lossy(&take_len_bytes(payload, &mut off)?).into_owned()
    } else {
        String::new()
    };
    Ok(ShvRequest { op, name, value })
}

pub fn encode_shv_ok_value(value: &str) -> Vec<u8> {
    let mut p = 0i32.to_le_bytes().to_vec();
    push_len_bytes(&mut p, value.as_bytes());
    p
}

pub fn encode_shv_ok() -> Vec<u8> {
    0i32.to_le_bytes().to_vec()
}

pub fn encode_shv_ok_list(list: &[(String, String)]) -> Vec<u8> {
    let mut p = 0i32.to_le_bytes().to_vec();
    p.extend_from_slice(&(list.len() as u32).to_le_bytes());
    for (name, value) in list {
        push_len_bytes(&mut p, name.as_bytes());
        push_len_bytes(&mut p, value.as_bytes());
    }
    p
}

pub fn encode_shv_err(message: &str) -> Vec<u8> {
    let mut p = 8i32.to_le_bytes().to_vec();
    push_len_bytes(&mut p, message.as_bytes());
    p
}

pub fn decode_shv_reply(payload: &[u8], op: u8) -> io::Result<ShvReply> {
    if payload.len() < 4 {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "SHV reply is truncated",
        ));
    }
    let rc = i32::from_le_bytes(payload[..4].try_into().unwrap());
    let mut off = 4;
    if rc != 0 {
        let message = if off < payload.len() {
            String::from_utf8_lossy(&take_len_bytes(payload, &mut off).unwrap_or_default())
                .into_owned()
        } else {
            "variable pool request failed".into()
        };
        return Ok(ShvReply {
            ok: false,
            message,
            value: String::new(),
            list: Vec::new(),
        });
    }
    match op {
        SHV_GET => {
            let value = if off < payload.len() {
                String::from_utf8_lossy(&take_len_bytes(payload, &mut off)?).into_owned()
            } else {
                String::new()
            };
            Ok(ShvReply {
                ok: true,
                message: String::new(),
                value,
                list: Vec::new(),
            })
        }
        SHV_LIST => {
            if off + 4 > payload.len() {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "SHV list count is truncated",
                ));
            }
            let n = u32::from_le_bytes(payload[off..off + 4].try_into().unwrap()) as usize;
            off += 4;
            let mut list = Vec::with_capacity(n);
            for _ in 0..n {
                let name =
                    String::from_utf8_lossy(&take_len_bytes(payload, &mut off)?).into_owned();
                let value =
                    String::from_utf8_lossy(&take_len_bytes(payload, &mut off)?).into_owned();
                list.push((name, value));
            }
            Ok(ShvReply {
                ok: true,
                message: String::new(),
                value: String::new(),
                list,
            })
        }
        _ => Ok(ShvReply {
            ok: true,
            message: String::new(),
            value: String::new(),
            list: Vec::new(),
        }),
    }
}

pub fn decode_wait_any(payload: &[u8]) -> io::Result<Vec<u32>> {
    if payload.len() < 4 {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "WAIT_ANY payload is truncated",
        ));
    }
    let n = u32::from_le_bytes(payload[..4].try_into().unwrap()) as usize;
    let mut streams = Vec::with_capacity(n);
    let mut off = 4;
    for _ in 0..n {
        if off + 4 > payload.len() {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "WAIT_ANY stream list is truncated",
            ));
        }
        streams.push(u32::from_le_bytes(
            payload[off..off + 4].try_into().unwrap(),
        ));
        off += 4;
    }
    Ok(streams)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn frame_round_trip() {
        let mut buf = Vec::new();
        write_frame(&mut buf, RECORD, b"hello").unwrap();
        let frame = read_frame(&mut Cursor::new(&buf)).unwrap();
        assert_eq!(frame.typ, RECORD);
        assert_eq!(frame.payload, b"hello");
    }

    #[test]
    fn empty_payload_is_allowed() {
        let mut buf = Vec::new();
        write_frame(&mut buf, EOF, b"").unwrap();
        let frame = read_frame(&mut Cursor::new(&buf)).unwrap();
        assert_eq!(frame.typ, EOF);
        assert!(frame.payload.is_empty());
    }

    #[test]
    fn hello_encodes_path_and_args() {
        let p = encode_hello("examples/upper.py", &["a".into(), "b".into()]);
        assert_eq!(&p[0..2], &1u16.to_le_bytes());
        let path_len = u32::from_le_bytes(p[6..10].try_into().unwrap()) as usize;
        assert_eq!(&p[10..10 + path_len], b"examples/upper.py");
        let (path, args, meta) = decode_hello(&p).unwrap();
        assert_eq!(path, "examples/upper.py");
        assert_eq!(args, ["a", "b"]);
        assert_eq!(meta, HelloMeta::default());
    }

    #[test]
    fn hello_meta_round_trip() {
        let meta = HelloMeta {
            stage_num: 2,
            eof_report: 1,
            nocommit: true,
            connected_in: vec![0, 1],
            connected_out: vec![0],
        };
        let p = encode_hello_with_meta("s.rexx", &[], Some(&meta));
        let (path, args, got) = decode_hello(&p).unwrap();
        assert_eq!(path, "s.rexx");
        assert!(args.is_empty());
        assert_eq!(got, meta);
    }

    #[test]
    fn hello_without_stream_lists_keeps_primary() {
        let mut p = encode_hello("s.py", &[]);
        p.extend_from_slice(&3u32.to_le_bytes());
        p.push(0);
        p.push(0);
        let (_, _, meta) = decode_hello(&p).unwrap();
        assert_eq!(meta.stage_num, 3);
        assert_eq!(meta.connected_in, [0]);
        assert_eq!(meta.connected_out, [0]);
    }

    #[test]
    fn callpipe_and_addstream_round_trip() {
        assert_eq!(
            decode_callpipe(&encode_callpipe("*: | copy | *:")).unwrap(),
            "*: | copy | *:"
        );
        assert_eq!(ADDPIPE, 0x1C);
        let p = encode_addstream(true, false, Some(2));
        assert_eq!(decode_addstream(&p).unwrap(), (true, false, Some(2)));
        assert_eq!(decode_wait_any(&encode_wait_any(&[0, 2])).unwrap(), [0, 2]);
    }

    #[test]
    fn stream_payload_defaults_to_zero() {
        assert_eq!(decode_stream_payload(b""), (0, b"".as_ref()));
        assert_eq!(decode_stream_payload(b"xy"), (0, b"xy".as_ref()));
        let p = encode_stream_payload(0, b"rec");
        assert_eq!(decode_stream_payload(&p), (0, b"rec".as_ref()));
    }

    #[test]
    fn ok_payload_accepts_stream_prefixed_and_bare_rc() {
        assert_eq!(decode_ok(&encode_ok_stream(0, 12)).unwrap(), 12);
        assert_eq!(decode_ok(&12i32.to_le_bytes()).unwrap(), 12);
    }

    #[test]
    fn short_side_round_trip() {
        let p = encode_short(0, ShortSide::Input);
        assert_eq!(decode_short(&p), (0, ShortSide::Input));
        assert_eq!(
            decode_short(&encode_stream_payload(0, b"")),
            (0, ShortSide::Both)
        );
    }

    #[test]
    fn shv_request_and_reply_round_trip() {
        let req = encode_shv_request(SHV_SET, "FOO", "bar");
        let decoded = decode_shv_request(&req).unwrap();
        assert_eq!(decoded.op, SHV_SET);
        assert_eq!(decoded.name, "FOO");
        assert_eq!(decoded.value, "bar");

        let get = decode_shv_reply(&encode_shv_ok_value("hello"), SHV_GET).unwrap();
        assert!(get.ok);
        assert_eq!(get.value, "hello");

        let list = decode_shv_reply(
            &encode_shv_ok_list(&[("A".into(), "1".into()), ("B".into(), "2".into())]),
            SHV_LIST,
        )
        .unwrap();
        assert_eq!(
            list.list,
            [("A".into(), "1".into()), ("B".into(), "2".into())]
        );

        let err = decode_shv_reply(&encode_shv_err("no pool"), SHV_GET).unwrap();
        assert!(!err.ok);
        assert_eq!(err.message, "no pool");
    }
}
