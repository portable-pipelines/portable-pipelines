//! SNAKE — build a multicolumn page layout.
//!
//! Author's Edition 1.1.11 pp.491–492 (walkthrough pp.16–17 Figure 27;
//! listpds example p.411).
//!
//! Syntax (fixed railroad order; diagram all-caps):
//! `SNAKE number [number]`
//!
//! The first number is the number of columns. The optional second number is
//! the page depth. When the depth is omitted, SNAKE reads the whole file and
//! uses the minimum number of rows that can hold the records (`ceil(N / C)`).
//! Columns are filled down, then across. SNAKE neither pads nor truncates.

use pipe_core::{Record, StageCtx, StreamId};

pub fn snake(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| {
        let spec = match parse_snake(&args) {
            Ok(spec) => spec,
            Err(e) => {
                ctx.fail(format!("SNAKE: {e}"));
                return;
            }
        };

        for stream in ctx.connected_inputs() {
            if stream.0 > StreamId::PRIMARY.0 {
                ctx.fail("SNAKE: no input stream other than the primary may be connected");
                return;
            }
        }

        if !ctx.output_connected(StreamId::PRIMARY) {
            ctx.select_input(StreamId::PRIMARY);
            ctx.short_input();
            return;
        }

        ctx.select_input(StreamId::PRIMARY);
        ctx.select_output(StreamId::PRIMARY);
        run_snake(ctx, spec);
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SnakeSpec {
    columns: usize,
    /// `None` = one page whose depth is `ceil(N / columns)`.
    depth: Option<usize>,
}

fn parse_snake(args: &[String]) -> Result<SnakeSpec, String> {
    if args.is_empty() {
        return Err("number of columns is required".into());
    }

    if !is_number_token(&args[0]) {
        return Err(unexpected_operand(&args[0]));
    }
    let columns = parse_positive_number(&args[0], "number of columns")?;

    let mut i = 1;
    let mut depth = None;
    if i < args.len() {
        if !is_number_token(&args[i]) {
            return Err(unexpected_operand(&args[i]));
        }
        depth = Some(parse_positive_number(&args[i], "page depth")?);
        i += 1;
    }

    if i < args.len() {
        return Err(format!("unexpected operand {}", args[i]));
    }

    Ok(SnakeSpec { columns, depth })
}

fn is_number_token(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

fn parse_positive_number(s: &str, what: &str) -> Result<usize, String> {
    if !is_number_token(s) {
        return Err(format!("{what} requires a nonnegative number, got {s}"));
    }
    let n = s
        .parse::<i64>()
        .map_err(|_| format!("{what} {s} is out of range"))?;
    if n < 0 || n > i32::MAX as i64 {
        return Err(format!("{what} must be 1 through {}, got {s}", i32::MAX));
    }
    if n == 0 {
        return Err(format!("{what} must be positive"));
    }
    Ok(n as usize)
}

fn unexpected_operand(arg: &str) -> String {
    let u = arg.to_ascii_uppercase();
    if u == "COLUMNS"
        || u == "WIDTH"
        || u == "PAD"
        || u == "SEPARATOR"
        || u == "ACROSS"
        || u == "DOWN"
        || u == "FIRST"
        || u == "LAST"
        || u == "ANYCASE"
        || u == "*"
    {
        format!("unexpected operand {arg}")
    } else {
        format!("unexpected operand {arg}")
    }
}

fn run_snake(ctx: &mut StageCtx, spec: SnakeSpec) {
    match spec.depth {
        Some(depth) => {
            let Some(page_size) = spec.columns.checked_mul(depth) else {
                ctx.fail("SNAKE: page is too large");
                return;
            };
            loop {
                if !ctx.output_connected(StreamId::PRIMARY) {
                    return;
                }
                let page = read_page(ctx, page_size);
                if page.is_empty() {
                    return;
                }
                if !write_page(ctx, &page, spec.columns, depth) {
                    return;
                }
            }
        }
        None => {
            let page = read_page(ctx, usize::MAX);
            if page.is_empty() {
                return;
            }
            let depth = page.len().div_ceil(spec.columns);
            let _ = write_page(ctx, &page, spec.columns, depth);
        }
    }
}

fn read_page(ctx: &mut StageCtx, max: usize) -> Vec<Record> {
    let mut page = Vec::new();
    while page.len() < max {
        match ctx.read() {
            Ok(Some(r)) => page.push(r),
            Ok(None) | Err(_) => break,
        }
    }
    page
}

/// Column-major layout: output row `r` is records `r`, `r+n`, `r+2n`, …
/// (1.1.11 p.491). Missing cells are omitted (neither padded nor truncated).
/// Trailing rows that contain no records are not written.
fn write_page(ctx: &mut StageCtx, page: &[Record], columns: usize, depth: usize) -> bool {
    for row in layout_page(page, columns, depth) {
        if !ctx.output_connected(StreamId::PRIMARY) {
            return false;
        }
        if ctx.write(Record::from(row)).is_err() {
            return false;
        }
    }
    true
}

fn layout_page(page: &[Record], columns: usize, depth: usize) -> Vec<Vec<u8>> {
    let mut rows = Vec::new();
    for row in 0..depth {
        let mut out = Vec::new();
        let mut any = false;
        for col in 0..columns {
            let idx = col * depth + row;
            if let Some(rec) = page.get(idx) {
                out.extend_from_slice(rec.as_bytes());
                any = true;
            }
        }
        if any {
            rows.push(out);
        }
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(args: &[&str]) -> SnakeSpec {
        parse_snake(&args.iter().map(|s| (*s).to_string()).collect::<Vec<_>>()).unwrap()
    }

    fn recs(lines: &[&str]) -> Vec<Record> {
        lines.iter().map(|s| Record::from(*s)).collect()
    }

    fn layout(lines: &[&str], columns: usize, depth: usize) -> Vec<String> {
        layout_page(&recs(lines), columns, depth)
            .into_iter()
            .map(|b| String::from_utf8(b).unwrap())
            .collect()
    }

    #[test]
    fn book_square_two_by_two() {
        // p.492: a b c d into 2 columns, depth omitted → ceil(4/2)=2.
        assert_eq!(layout(&["a", "b", "c", "d"], 2, 2), vec!["ac", "bd"]);
    }

    #[test]
    fn specified_depth_last_page_keeps_n() {
        // C=2, D=3, N=5: records 1, n+1 on the first output line.
        assert_eq!(
            layout(&["1", "2", "3", "4", "5"], 2, 3),
            vec!["14", "25", "3"]
        );
    }

    #[test]
    fn omitted_depth_is_ceil() {
        assert_eq!(
            layout(&["1", "2", "3", "4", "5"], 2, 3),
            vec!["14", "25", "3"]
        );
        // 5 records / 3 columns → depth 2; last column incomplete.
        assert_eq!(layout(&["1", "2", "3", "4", "5"], 3, 2), vec!["135", "24"]);
    }

    #[test]
    fn neither_pads_nor_truncates() {
        assert_eq!(layout(&["aa", "b", "ccc", "d"], 2, 2), vec!["aaccc", "bd"]);
    }

    #[test]
    fn null_cell_contributes_no_bytes() {
        assert_eq!(layout(&["a", "", "c"], 3, 1), vec!["ac"]);
    }

    #[test]
    fn parse_requires_positive_columns() {
        assert!(parse_snake(&[])
            .unwrap_err()
            .contains("columns is required"));
        assert!(parse_snake(&["0".into()])
            .unwrap_err()
            .contains("must be positive"));
        assert!(parse_snake(&["2".into(), "0".into()])
            .unwrap_err()
            .contains("must be positive"));
    }

    #[test]
    fn parse_rejects_undocumented() {
        let err = parse_snake(&["columns".into(), "2".into()]).unwrap_err();
        assert!(err.contains("unexpected operand"), "{err}");
        let err = parse_snake(&["2".into(), "pad".into()]).unwrap_err();
        assert!(err.contains("unexpected operand"), "{err}");
        let err = parse_snake(&["2".into(), "3".into(), "4".into()]).unwrap_err();
        assert!(err.contains("unexpected operand"), "{err}");
    }

    #[test]
    fn parse_accepts_two_numbers() {
        assert_eq!(
            spec(&["6"]),
            SnakeSpec {
                columns: 6,
                depth: None
            }
        );
        assert_eq!(
            spec(&["2", "3"]),
            SnakeSpec {
                columns: 2,
                depth: Some(3)
            }
        );
    }
}
