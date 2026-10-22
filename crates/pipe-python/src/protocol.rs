//! Little-endian framed protocol shared with `pipelines._protocol`.

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub typ: u8,
    pub payload: Vec<u8>,
}

pub fn write_frame<W: Write>(w: &mut W, typ: u8, payload: &[u8]) -> io::Result<()> {
    if payload.len() > MAX_PAYLOAD as usize {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "python stage payload exceeds 16 MiB",
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
            "python stage payload exceeds 16 MiB",
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
    p
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

pub fn encode_ok(rc: i32) -> Vec<u8> {
    rc.to_le_bytes().to_vec()
}

pub fn encode_err(rc: i32, message: &str) -> Vec<u8> {
    let mut p = rc.to_le_bytes().to_vec();
    p.extend_from_slice(message.as_bytes());
    p
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
    }
}
