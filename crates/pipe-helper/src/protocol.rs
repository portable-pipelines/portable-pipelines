//! Little-endian framed protocol for out-of-process language stages.
//!
//! Layout: `u32le payload_len`, type byte, payload. I/O requests and
//! record/EOF/OK replies carry a `u32le` stream number (default 0) so a
//! later increment can select secondary streams without changing the frame
//! header. Only stream 0 is honoured today.

use std::io::{self, Read, Write};

pub const MAX_PAYLOAD: u32 = 16 * 1024 * 1024;

pub const HELLO: u8 = 0x01;
pub const READTO: u8 = 0x10;
pub const PEEKTO: u8 = 0x11;
pub const OUTPUT: u8 = 0x12;
pub const SHORT: u8 = 0x13;
pub const EXIT: u8 = 0x14;
pub const FAIL: u8 = 0x15;

pub const HELLO_OK: u8 = 0x80;
pub const RECORD: u8 = 0x81;
pub const EOF: u8 = 0x82;
pub const OK: u8 = 0x83;
pub const ERR: u8 = 0x84;

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
}

impl Default for HelloMeta {
    fn default() -> Self {
        Self {
            stage_num: 1,
            eof_report: 0,
            nocommit: false,
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
        };
        let p = encode_hello_with_meta("s.rexx", &[], Some(&meta));
        let (path, args, got) = decode_hello(&p).unwrap();
        assert_eq!(path, "s.rexx");
        assert!(args.is_empty());
        assert_eq!(got, meta);
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
}
