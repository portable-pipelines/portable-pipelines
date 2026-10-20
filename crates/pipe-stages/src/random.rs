//! RANDOM — generate pseudo-random numbers.
//!
//! Author's Edition 1.1.11 pp.469–470.
//!
//! Syntax (diagram all-caps): `RANDOM [number | *] [snumber | *]`
//!
//! Device driver; must be a first stage. Writes 4-byte big-endian binary
//! records until the output severs. The book does not document the generator;
//! this port uses SplitMix64. Tests must supply a seed.

use std::time::{SystemTime, UNIX_EPOCH};

use pipe_core::{Record, StageCtx, StreamId};

use crate::xrange::{is_snumber_token, parse_snumber};

/// SplitMix64 (Steele / Lea / Vigna). Not IBM’s undocumented CMS generator.
struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E3779B97F4A7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
        z ^ (z >> 31)
    }

    fn next_u32(&mut self) -> u32 {
        self.next_u64() as u32
    }
}

#[derive(Clone, Copy, Debug)]
struct RandomSpec {
    modulus: Option<u32>,
    seed: Option<i32>,
}

pub fn random(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| {
        let spec = match parse_random(&args) {
            Ok(spec) => spec,
            Err(e) => {
                ctx.fail(format!("RANDOM: {e}"));
                return;
            }
        };

        if ctx.input_connected(StreamId::PRIMARY) {
            ctx.fail("RANDOM must be a first stage");
            return;
        }

        if !ctx.output_connected(StreamId::PRIMARY) {
            return;
        }

        ctx.select_output(StreamId::PRIMARY);
        run_random(ctx, spec);
    })
}

fn parse_random(args: &[String]) -> Result<RandomSpec, String> {
    if args.len() > 2 {
        return Err(format!("unexpected operand {}", args[2]));
    }
    let mut spec = RandomSpec {
        modulus: None,
        seed: None,
    };
    if args.is_empty() {
        return Ok(spec);
    }
    spec.modulus = parse_modulus(&args[0])?;
    if args.len() == 2 {
        spec.seed = parse_seed(&args[1])?;
    }
    Ok(spec)
}

fn parse_modulus(s: &str) -> Result<Option<u32>, String> {
    if s == "*" {
        return Ok(None);
    }
    if !s.bytes().all(|b| b.is_ascii_digit()) || s.is_empty() {
        return Err(format!("unrecognized operand {s}"));
    }
    let n = s
        .parse::<i64>()
        .map_err(|_| format!("modulus {s} is out of range"))?;
    if n <= 0 || n > i32::MAX as i64 {
        return Err(format!(
            "modulus must be positive and at most {}, got {s}",
            i32::MAX
        ));
    }
    Ok(Some(n as u32))
}

fn parse_seed(s: &str) -> Result<Option<i32>, String> {
    if s == "*" {
        return Ok(None);
    }
    if !is_snumber_token(s) {
        return Err(format!("unrecognized operand {s}"));
    }
    Ok(Some(parse_snumber(s)?))
}

fn seed_bits(seed: Option<i32>) -> u64 {
    match seed {
        Some(n) => n as i64 as u64,
        None => {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default();
            now.as_nanos() as u64 ^ now.as_secs().wrapping_mul(0x9E37)
        }
    }
}

fn next_value(rng: &mut SplitMix64, modulus: Option<u32>) -> u32 {
    let raw = rng.next_u32();
    match modulus {
        Some(m) => raw % m,
        None => raw,
    }
}

fn run_random(ctx: &mut StageCtx, spec: RandomSpec) {
    let mut rng = SplitMix64::new(seed_bits(spec.seed));
    while ctx.output_connected(StreamId::PRIMARY) {
        let value = next_value(&mut rng, spec.modulus);
        if ctx
            .write(Record::from(value.to_be_bytes().to_vec()))
            .is_err()
        {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(args: &[&str]) -> RandomSpec {
        parse_random(&args.iter().map(|s| (*s).to_string()).collect::<Vec<_>>()).unwrap()
    }

    #[test]
    fn operands() {
        let a = spec(&[]);
        assert!(a.modulus.is_none() && a.seed.is_none());
        let b = spec(&["*"]);
        assert!(b.modulus.is_none() && b.seed.is_none());
        let c = spec(&["7"]);
        assert_eq!(c.modulus, Some(7));
        assert!(c.seed.is_none());
        let d = spec(&["*", "42"]);
        assert!(d.modulus.is_none());
        assert_eq!(d.seed, Some(42));
        let e = spec(&["7", "-5"]);
        assert_eq!(e.modulus, Some(7));
        assert_eq!(e.seed, Some(-5));
        let f = spec(&["*", "*"]);
        assert!(f.modulus.is_none() && f.seed.is_none());
    }

    #[test]
    fn modulus_must_be_positive() {
        assert!(parse_random(&["0".into()]).is_err());
        assert!(parse_random(&["-1".into()]).is_err());
        assert!(parse_random(&["7".into(), "x".into()]).is_err());
        assert!(parse_random(&["7".into(), "1".into(), "2".into()]).is_err());
    }

    #[test]
    fn splitmix_is_deterministic() {
        let mut a = SplitMix64::new(1);
        let mut b = SplitMix64::new(1);
        let xs: Vec<u32> = (0..5).map(|_| a.next_u32()).collect();
        let ys: Vec<u32> = (0..5).map(|_| b.next_u32()).collect();
        assert_eq!(xs, ys);
        assert!(xs.iter().any(|&v| v != xs[0]));
        let mut c = SplitMix64::new(2);
        let zs: Vec<u32> = (0..5).map(|_| c.next_u32()).collect();
        assert_ne!(xs, zs);
    }

    #[test]
    fn remainder_is_below_modulus() {
        let mut rng = SplitMix64::new(99);
        for _ in 0..50 {
            assert!(next_value(&mut rng, Some(7)) < 7);
        }
    }
}
