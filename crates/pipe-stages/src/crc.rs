//! CRC — checksum of concatenated input records.
//!
//! z/VM CMS Pipelines Reference (CRC). The documented algorithm is CRC16I:
//! 16-bit, polynomial 0x8005, initial value 0x8000, output XOR 0x0000.
//! Record boundaries have no effect. The checksum is written as two
//! big-endian bytes.
//!
//! `CRC [APPEND] [CRC16I]`. Without APPEND and without a secondary output,
//! one checksum record is written to the primary. APPEND copies primary
//! input to primary output and then writes the checksum. A connected
//! secondary output receives the checksum; primary then copies the input.
//! APPEND cannot be combined with a secondary output.
//!
//! This is a portable subset: the bit-orientation is MSB-first and has not
//! been checked against an IBM CRC16I result.

use pipe_core::{Record, StageCtx, StreamId};

const POLY: u16 = 0x8005;
const INIT: u16 = 0x8000;
const XOROUT: u16 = 0x0000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Spec {
    append: bool,
}

pub fn crc(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| {
        let spec = match parse_crc(&args) {
            Ok(s) => s,
            Err(e) => {
                ctx.fail(format!("CRC: {e}"));
                return;
            }
        };
        for stream in ctx.connected_inputs() {
            if stream != StreamId::PRIMARY {
                ctx.fail("CRC: the secondary input stream must not be connected");
                return;
            }
        }
        let secondary_out = ctx.output_connected(StreamId::SECONDARY);
        if spec.append && secondary_out {
            ctx.fail("CRC: APPEND cannot be specified when the secondary output is connected");
            return;
        }
        run_crc(ctx, spec.append, secondary_out);
    })
}

fn parse_crc(args: &[String]) -> Result<Spec, String> {
    let mut append = false;
    let mut saw_algo = false;
    for tok in args {
        let u = tok.to_ascii_uppercase();
        if u == "APPEND" {
            if append {
                return Err("APPEND is specified more than once".into());
            }
            append = true;
        } else if u == "CRC16I" {
            saw_algo = true;
        } else {
            return Err(format!("unrecognized operand {tok}"));
        }
    }
    let _ = saw_algo;
    Ok(Spec { append })
}

fn run_crc(ctx: &mut StageCtx, append: bool, secondary_out: bool) {
    let primary_out = ctx.output_connected(StreamId::PRIMARY);
    if append && !primary_out {
        ctx.select_input(StreamId::PRIMARY);
        ctx.short_input();
        return;
    }
    if !append && !primary_out && !secondary_out {
        ctx.select_input(StreamId::PRIMARY);
        ctx.short_input();
        return;
    }

    ctx.select_input(StreamId::PRIMARY);
    let mut crc = INIT;
    loop {
        if append && !ctx.output_connected(StreamId::PRIMARY) {
            return;
        }
        match ctx.read() {
            Ok(Some(rec)) => {
                crc = crc16i_update(crc, rec.as_bytes());
                if (append || secondary_out) && ctx.output_connected(StreamId::PRIMARY) {
                    ctx.select_output(StreamId::PRIMARY);
                    if ctx.write(rec).is_err() {
                        return;
                    }
                }
            }
            Ok(None) => {
                let out = crc_record(crc);
                let dest = if secondary_out {
                    StreamId::SECONDARY
                } else {
                    StreamId::PRIMARY
                };
                if ctx.output_connected(dest) {
                    ctx.select_output(dest);
                    let _ = ctx.write(Record::from(out));
                }
                return;
            }
            Err(_) => return,
        }
    }
}

/// MSB-first CRC-16 with polynomial 0x8005.
pub(crate) fn crc16i_update(mut crc: u16, data: &[u8]) -> u16 {
    for &b in data {
        crc ^= u16::from(b) << 8;
        for _ in 0..8 {
            if crc & 0x8000 != 0 {
                crc = (crc << 1) ^ POLY;
            } else {
                crc <<= 1;
            }
        }
    }
    crc
}

#[allow(dead_code)]
pub(crate) fn crc16i(data: &[u8]) -> u16 {
    crc16i_finish(crc16i_update(INIT, data))
}

fn crc_record(crc: u16) -> Vec<u8> {
    crc16i_finish(crc).to_be_bytes().to_vec()
}

fn crc16i_finish(crc: u16) -> u16 {
    crc ^ XOROUT
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_is_initial_value() {
        assert_eq!(crc16i(b""), INIT);
        assert_eq!(crc_record(INIT), vec![0x80, 0x00]);
    }

    #[test]
    fn concatenating_records_matches_one_buffer() {
        let a = crc16i_update(INIT, b"ab");
        let b = crc16i_update(a, b"c");
        assert_eq!(b ^ XOROUT, crc16i(b"abc"));
    }

    #[test]
    fn parse_operands() {
        assert!(!parse_crc(&[]).unwrap().append);
        assert!(parse_crc(&["APPEND".into()]).unwrap().append);
        assert!(
            parse_crc(&["crc16i".into(), "append".into()])
                .unwrap()
                .append
        );
        assert!(parse_crc(&["nope".into()]).is_err());
    }
}
