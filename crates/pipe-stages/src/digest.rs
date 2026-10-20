//! DIGEST — compute a message digest.
//!
//! Author's Edition 1.1.11 pp.326–327.
//!
//! Syntax (diagram all-caps):
//! `DIGEST SHA1 | SHA256 | SHA384 | SHA512 | MD5 [APPEND | VERIFY]`
//!
//! Software hashes via RustCrypto (RFC 3174 / FIPS 180-2 / RFC 1321). CMS
//! Message-security Assist hardware (Notes 2–3) is not used.

use md5::Md5;
use sha1::{Digest, Sha1};
use sha2::{Sha256, Sha384, Sha512};

use pipe_core::{Record, StageCtx, StreamId};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Algo {
    Sha1,
    Sha256,
    Sha384,
    Sha512,
    Md5,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    /// Hash the whole primary file (or each SI-delimited batch).
    File,
    /// Append the digest of each record to that record.
    Append,
    /// Verify an APPEND-shaped record; drop padding+digest on success.
    Verify,
}

#[derive(Clone, Copy, Debug)]
struct DigestSpec {
    algo: Algo,
    mode: Mode,
}

enum Hasher {
    Sha1(Sha1),
    Sha256(Sha256),
    Sha384(Sha384),
    Sha512(Sha512),
    Md5(Md5),
}

impl Algo {
    fn digest_len(self) -> usize {
        match self {
            Algo::Sha1 => 20,
            Algo::Sha256 => 32,
            Algo::Sha384 => 48,
            Algo::Sha512 => 64,
            Algo::Md5 => 16,
        }
    }

    fn hasher(self) -> Hasher {
        match self {
            Algo::Sha1 => Hasher::Sha1(Sha1::new()),
            Algo::Sha256 => Hasher::Sha256(Sha256::new()),
            Algo::Sha384 => Hasher::Sha384(Sha384::new()),
            Algo::Sha512 => Hasher::Sha512(Sha512::new()),
            Algo::Md5 => Hasher::Md5(Md5::new()),
        }
    }

    fn hash(self, data: &[u8]) -> Vec<u8> {
        let mut h = self.hasher();
        h.update(data);
        h.finalize()
    }
}

impl Hasher {
    fn update(&mut self, data: &[u8]) {
        match self {
            Hasher::Sha1(h) => h.update(data),
            Hasher::Sha256(h) => h.update(data),
            Hasher::Sha384(h) => h.update(data),
            Hasher::Sha512(h) => h.update(data),
            Hasher::Md5(h) => h.update(data),
        }
    }

    fn finalize(self) -> Vec<u8> {
        match self {
            Hasher::Sha1(h) => h.finalize().to_vec(),
            Hasher::Sha256(h) => h.finalize().to_vec(),
            Hasher::Sha384(h) => h.finalize().to_vec(),
            Hasher::Sha512(h) => h.finalize().to_vec(),
            Hasher::Md5(h) => h.finalize().to_vec(),
        }
    }

    fn finalize_reset(&mut self) -> Vec<u8> {
        match self {
            Hasher::Sha1(h) => h.finalize_reset().to_vec(),
            Hasher::Sha256(h) => h.finalize_reset().to_vec(),
            Hasher::Sha384(h) => h.finalize_reset().to_vec(),
            Hasher::Sha512(h) => h.finalize_reset().to_vec(),
            Hasher::Md5(h) => h.finalize_reset().to_vec(),
        }
    }
}

pub fn digest(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| {
        let spec = match parse_digest(&args) {
            Ok(spec) => spec,
            Err(e) => {
                ctx.fail(format!("DIGEST: {e}"));
                return;
            }
        };

        for stream in ctx.connected_inputs() {
            if stream.0 > StreamId::SECONDARY.0 {
                ctx.fail(
                    "DIGEST: no input stream other than the primary and secondary may be connected",
                );
                return;
            }
        }

        match spec.mode {
            Mode::Append => {
                if ctx.input_connected(StreamId::SECONDARY)
                    || ctx.output_connected(StreamId::SECONDARY)
                {
                    ctx.fail(
                        "DIGEST: secondary streams must not be defined when APPEND is specified",
                    );
                    return;
                }
            }
            Mode::Verify => {
                if ctx.input_connected(StreamId::SECONDARY) {
                    ctx.fail("DIGEST: the secondary input stream must not be connected when VERIFY is specified");
                    return;
                }
            }
            Mode::File => {}
        }

        match spec.mode {
            Mode::Append => run_append(ctx, spec.algo),
            Mode::Verify => run_verify(ctx, spec.algo),
            Mode::File => {
                if ctx.input_connected(StreamId::SECONDARY)
                    || ctx.output_connected(StreamId::SECONDARY)
                {
                    run_batch(ctx, spec.algo);
                } else {
                    run_file(ctx, spec.algo);
                }
            }
        }
    })
}

fn parse_digest(args: &[String]) -> Result<DigestSpec, String> {
    if args.is_empty() {
        return Err("an algorithm is required (SHA1, SHA256, SHA384, SHA512, or MD5)".into());
    }
    let algo = parse_algo(&args[0])?;
    let mut mode = Mode::File;
    if args.len() >= 2 {
        mode = parse_mode(&args[1])?;
    }
    if args.len() > 2 {
        return Err(format!("unexpected operand {}", args[2]));
    }
    Ok(DigestSpec { algo, mode })
}

fn parse_algo(s: &str) -> Result<Algo, String> {
    match s.to_ascii_uppercase().as_str() {
        "SHA1" => Ok(Algo::Sha1),
        "SHA256" => Ok(Algo::Sha256),
        "SHA384" => Ok(Algo::Sha384),
        "SHA512" => Ok(Algo::Sha512),
        "MD5" => Ok(Algo::Md5),
        _ => Err(format!("unrecognized operand {s}")),
    }
}

fn parse_mode(s: &str) -> Result<Mode, String> {
    match s.to_ascii_uppercase().as_str() {
        "APPEND" => Ok(Mode::Append),
        "VERIFY" => Ok(Mode::Verify),
        _ => Err(format!("unrecognized operand {s}")),
    }
}

fn short_if_no_primary(ctx: &mut StageCtx) -> bool {
    if ctx.output_connected(StreamId::PRIMARY) {
        return false;
    }
    ctx.select_input(StreamId::PRIMARY);
    ctx.short_input();
    true
}

/// Concatenate primary records (no inserted separators) and write one digest.
fn run_file(ctx: &mut StageCtx, algo: Algo) {
    if short_if_no_primary(ctx) {
        return;
    }
    ctx.select_input(StreamId::PRIMARY);
    ctx.select_output(StreamId::PRIMARY);
    let mut hasher = algo.hasher();
    loop {
        if !ctx.output_connected(StreamId::PRIMARY) {
            return;
        }
        match ctx.read() {
            Ok(Some(rec)) => hasher.update(rec.as_bytes()),
            Ok(None) => {
                let _ = ctx.write(Record::from(hasher.finalize()));
                return;
            }
            Err(_) => return,
        }
    }
}

/// Per-record: write `record || digest(record)`. No extra stored pad bytes.
fn run_append(ctx: &mut StageCtx, algo: Algo) {
    if short_if_no_primary(ctx) {
        return;
    }
    ctx.select_input(StreamId::PRIMARY);
    ctx.select_output(StreamId::PRIMARY);
    loop {
        if !ctx.output_connected(StreamId::PRIMARY) {
            return;
        }
        if ctx.input_record_ready(StreamId::PRIMARY) {
            let Some(rec) = ctx.peek() else {
                continue;
            };
            let mut out = rec.as_bytes().to_vec();
            out.extend_from_slice(&algo.hash(rec.as_bytes()));
            if ctx.write(Record::from(out)).is_err() {
                return;
            }
            let _ = ctx.read();
            continue;
        }
        if ctx.input_at_eof(StreamId::PRIMARY) {
            return;
        }
        if ctx.wait_any_input(&[StreamId::PRIMARY]).is_err() {
            return;
        }
    }
}

/// VERIFY: last `digest_len` bytes are the digest of the prefix (p.326).
fn run_verify(ctx: &mut StageCtx, algo: Algo) {
    let primary_out = ctx.output_connected(StreamId::PRIMARY);
    let secondary_out = ctx.output_connected(StreamId::SECONDARY);
    if !primary_out && !secondary_out {
        ctx.select_input(StreamId::PRIMARY);
        ctx.short_input();
        return;
    }
    ctx.select_input(StreamId::PRIMARY);
    let n = algo.digest_len();
    loop {
        if primary_out && !ctx.output_connected(StreamId::PRIMARY) {
            return;
        }
        if secondary_out && !ctx.output_connected(StreamId::SECONDARY) {
            return;
        }
        if ctx.input_record_ready(StreamId::PRIMARY) {
            let Some(rec) = ctx.peek() else {
                continue;
            };
            let bytes = rec.as_bytes();
            let ok = bytes.len() >= n
                && algo.hash(&bytes[..bytes.len() - n]) == bytes[bytes.len() - n..];
            let dest = if ok {
                StreamId::PRIMARY
            } else {
                StreamId::SECONDARY
            };
            let out = if ok {
                Record::from(bytes[..bytes.len() - n].to_vec())
            } else {
                rec.clone()
            };
            if ctx.output_connected(dest) {
                ctx.select_output(dest);
                if ctx.write(out).is_err() {
                    return;
                }
            }
            let _ = ctx.read();
            continue;
        }
        if ctx.input_at_eof(StreamId::PRIMARY) {
            return;
        }
        if ctx.wait_any_input(&[StreamId::PRIMARY]).is_err() {
            return;
        }
    }
}

/// Secondary streams: copy PI → SO; write digest on PO at SI trigger or PI EOF.
fn run_batch(ctx: &mut StageCtx, algo: Algo) {
    let primary_out = ctx.output_connected(StreamId::PRIMARY);
    let secondary_out = ctx.output_connected(StreamId::SECONDARY);
    if !primary_out && !secondary_out {
        ctx.select_input(StreamId::PRIMARY);
        ctx.short_input();
        if ctx.input_connected(StreamId::SECONDARY) {
            ctx.select_input(StreamId::SECONDARY);
            ctx.short_input();
        }
        return;
    }

    let mut hasher = algo.hasher();
    loop {
        if primary_out && !ctx.output_connected(StreamId::PRIMARY) {
            return;
        }
        if secondary_out && !ctx.output_connected(StreamId::SECONDARY) {
            return;
        }

        if ctx.input_record_ready(StreamId::PRIMARY) {
            let Some(rec) = ctx.peek() else {
                continue;
            };
            hasher.update(rec.as_bytes());
            if secondary_out && ctx.output_connected(StreamId::SECONDARY) {
                ctx.select_output(StreamId::SECONDARY);
                if ctx.write(rec.clone()).is_err() {
                    return;
                }
            }
            ctx.select_input(StreamId::PRIMARY);
            let _ = ctx.read();
            continue;
        }

        if ctx.input_record_ready(StreamId::SECONDARY) {
            if primary_out && ctx.output_connected(StreamId::PRIMARY) {
                ctx.select_output(StreamId::PRIMARY);
                if ctx.write(Record::from(hasher.finalize_reset())).is_err() {
                    return;
                }
            } else {
                hasher = algo.hasher();
            }
            ctx.select_input(StreamId::SECONDARY);
            let _ = ctx.read();
            continue;
        }

        if ctx.input_at_eof(StreamId::PRIMARY) {
            if primary_out && ctx.output_connected(StreamId::PRIMARY) {
                ctx.select_output(StreamId::PRIMARY);
                let _ = ctx.write(Record::from(hasher.finalize()));
            }
            return;
        }

        if ctx
            .wait_any_input(&[StreamId::PRIMARY, StreamId::SECONDARY])
            .is_err()
        {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(args: &[&str]) -> DigestSpec {
        parse_digest(&args.iter().map(|s| (*s).to_string()).collect::<Vec<_>>()).unwrap()
    }

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02X}")).collect()
    }

    #[test]
    fn parse_algorithms_and_modes() {
        assert_eq!(spec(&["sha1"]).algo, Algo::Sha1);
        assert_eq!(spec(&["SHA256", "append"]).mode, Mode::Append);
        assert_eq!(spec(&["md5", "VERIFY"]).mode, Mode::Verify);
        assert!(parse_digest(&[]).is_err());
        assert!(parse_digest(&["sha".into()]).is_err());
        assert!(parse_digest(&["sha1".into(), "sign".into()]).is_err());
        assert!(parse_digest(&["sha1".into(), "append".into(), "verify".into()]).is_err());
    }

    #[test]
    fn rfc3174_sha1_abc() {
        // RFC 3174 / FIPS 180-1: SHA1("abc")
        assert_eq!(
            hex(&Algo::Sha1.hash(b"abc")),
            "A9993E364706816ABA3E25717850C26C9CD0D89D"
        );
    }

    #[test]
    fn rfc1321_md5_vectors() {
        assert_eq!(
            hex(&Algo::Md5.hash(b"")),
            "D41D8CD98F00B204E9800998ECF8427E"
        );
        assert_eq!(
            hex(&Algo::Md5.hash(b"a")),
            "0CC175B9C0F1B6A831C399E269772661"
        );
        assert_eq!(
            hex(&Algo::Md5.hash(b"abc")),
            "900150983CD24FB0D6963F7D28E17F72"
        );
    }

    #[test]
    fn fips180_sha2_abc() {
        assert_eq!(
            hex(&Algo::Sha256.hash(b"abc")),
            "BA7816BF8F01CFEA414140DE5DAE2223B00361A396177A9CB410FF61F20015AD"
        );
        assert_eq!(
            hex(&Algo::Sha384.hash(b"abc")),
            "CB00753F45A35E8BB5A03D699AC65007272C32AB0EDED1631A8B605A43FF5BED\
             8086072BA1E7CC2358BAECA134C825A7"
        );
        assert_eq!(
            hex(&Algo::Sha512.hash(b"abc")),
            "DDAF35A193617ABACC417349AE20413112E6FA4E89A97EA20A9EEEE64B55D39A\
             2192992A274FC1A836BA3C23A3FEEBBD454D4423643CE80E2A9AC94FA54CA49F"
        );
    }

    #[test]
    fn append_is_record_plus_digest() {
        let rec = b"abc";
        let mut out = rec.to_vec();
        out.extend_from_slice(&Algo::Sha1.hash(rec));
        assert_eq!(&out[..3], rec);
        assert_eq!(&out[3..], Algo::Sha1.hash(rec));
        assert_eq!(out.len(), 23);
    }
}
