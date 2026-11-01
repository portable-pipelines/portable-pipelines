//! Protocol client used by the Regina helper process.

use std::fs::File;
use std::io;

use pipe_helper::protocol::{
    decode_ok, decode_stream_payload, encode_addstream, encode_callpipe, encode_commit,
    encode_exit, encode_short, encode_stream_payload, encode_streamstate, encode_wait_any,
    read_frame, write_frame, Frame, ShortSide, ADDSTREAM, CALLPIPE, COMMIT, EOF, ERR, EXIT, FAIL,
    OK, OUTPUT, PEEKTO, READTO, RECORD, STREAMSTATE, WAIT_ANY,
};

pub struct ProtocolConn {
    pub inp: File,
    pub out: File,
}

impl ProtocolConn {
    pub fn from_files(inp: File, out: File) -> Self {
        Self { inp, out }
    }

    fn request(&mut self, typ: u8, payload: &[u8]) -> Result<Frame, String> {
        write_frame(&mut self.out, typ, payload).map_err(|e| e.to_string())?;
        read_frame(&mut self.inp).map_err(|e| e.to_string())
    }

    pub fn readto(&mut self, stream: u32) -> Result<Option<Vec<u8>>, String> {
        self.read_record(READTO, stream)
    }

    pub fn peekto(&mut self, stream: u32) -> Result<Option<Vec<u8>>, String> {
        self.read_record(PEEKTO, stream)
    }

    fn read_record(&mut self, typ: u8, stream: u32) -> Result<Option<Vec<u8>>, String> {
        let frame = self.request(typ, &encode_stream_payload(stream, &[]))?;
        match frame.typ {
            RECORD => {
                let (_s, data) = decode_stream_payload(&frame.payload);
                Ok(Some(data.to_vec()))
            }
            EOF => Ok(None),
            ERR => Err(err_message(&frame.payload)),
            other => Err(format!("unexpected reply 0x{other:02x} for record request")),
        }
    }

    pub fn output(&mut self, stream: u32, data: &[u8]) -> Result<i32, String> {
        let frame = self.request(OUTPUT, &encode_stream_payload(stream, data))?;
        match frame.typ {
            OK => decode_ok(&frame.payload).map_err(|e| e.to_string()),
            ERR => Err(err_message(&frame.payload)),
            other => Err(format!("unexpected OUTPUT reply 0x{other:02x}")),
        }
    }

    pub fn short(&mut self, stream: u32, side: ShortSide) -> Result<i32, String> {
        let frame = self.request(pipe_helper::protocol::SHORT, &encode_short(stream, side))?;
        match frame.typ {
            OK => decode_ok(&frame.payload).map_err(|e| e.to_string()),
            ERR => Err(err_message(&frame.payload)),
            other => Err(format!("unexpected SHORT reply 0x{other:02x}")),
        }
    }

    pub fn send_exit(&mut self, rc: i32) -> io::Result<()> {
        write_frame(&mut self.out, EXIT, &encode_exit(rc))
    }

    pub fn send_fail(&mut self, message: &str) -> io::Result<()> {
        write_frame(&mut self.out, FAIL, message.as_bytes())
    }

    pub fn callpipe(&mut self, spec: &str) -> Result<i32, String> {
        let frame = self.request(CALLPIPE, &encode_callpipe(spec))?;
        match frame.typ {
            OK => decode_ok(&frame.payload).map_err(|e| e.to_string()),
            ERR => Err(err_message(&frame.payload)),
            other => Err(format!("unexpected CALLPIPE reply 0x{other:02x}")),
        }
    }

    pub fn commit(&mut self, level: i32) -> Result<i32, String> {
        let frame = self.request(COMMIT, &encode_commit(level))?;
        match frame.typ {
            OK => decode_ok(&frame.payload).map_err(|e| e.to_string()),
            ERR => Err(err_message(&frame.payload)),
            other => Err(format!("unexpected COMMIT reply 0x{other:02x}")),
        }
    }

    pub fn addstream(
        &mut self,
        input: bool,
        output: bool,
        number: Option<u32>,
    ) -> Result<i32, String> {
        let frame = self.request(ADDSTREAM, &encode_addstream(input, output, number))?;
        match frame.typ {
            OK => decode_ok(&frame.payload).map_err(|e| e.to_string()),
            ERR => Err(err_message(&frame.payload)),
            other => Err(format!("unexpected ADDSTREAM reply 0x{other:02x}")),
        }
    }

    pub fn streamstate(&mut self, side_input: bool, stream: u32) -> Result<i32, String> {
        let frame = self.request(STREAMSTATE, &encode_streamstate(side_input, stream))?;
        match frame.typ {
            OK => decode_ok(&frame.payload).map_err(|e| e.to_string()),
            ERR => Err(err_message(&frame.payload)),
            other => Err(format!("unexpected STREAMSTATE reply 0x{other:02x}")),
        }
    }

    pub fn wait_any(&mut self, streams: &[u32]) -> Result<Option<u32>, String> {
        let frame = self.request(WAIT_ANY, &encode_wait_any(streams))?;
        match frame.typ {
            OK => {
                if frame.payload.len() >= 4 {
                    Ok(Some(u32::from_le_bytes(
                        frame.payload[..4].try_into().unwrap(),
                    )))
                } else {
                    Ok(Some(0))
                }
            }
            EOF => Ok(None),
            ERR => Err(err_message(&frame.payload)),
            other => Err(format!("unexpected WAIT_ANY reply 0x{other:02x}")),
        }
    }
}

fn err_message(payload: &[u8]) -> String {
    if payload.len() >= 4 {
        String::from_utf8_lossy(&payload[4..]).into_owned()
    } else {
        "protocol ERR".into()
    }
}
