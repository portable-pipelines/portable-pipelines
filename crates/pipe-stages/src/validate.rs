//! Operand checkers for the GUI. They parse operands without running a stage.
//!
//! A missing checker is `None` (no squiggle). REXX and Python stages are
//! never checked.

use pipe_parser::operands_for_stage;

use crate::builtins::{check_change, check_console_read, check_count, check_ends, check_locate};
use crate::file_io::file_path;
use crate::registry::StageRegistry;
use crate::spec::check_spec;
use crate::topology::is_language_stage_verb;

const FILE_VERBS: &[&str] = &[
    "<",
    ">",
    ">>",
    "DISK",
    "FILEBACK",
    "DISKBACK",
    "FILERAND",
    "DISKRAND",
    "FILEFAST",
    "DISKFAST",
    "FILESLOW",
    "DISKSLOW",
    "FILEUPDATE",
    "DISKUPDATE",
];

/// Check operands without running the stage.
///
/// `None` means this verb has no checker yet (or is a language stage).
/// `Some(Ok(()))` means the operands parsed. `Some(Err(message))` is shown
/// with the same squiggle as a Source parse error.
pub fn check_stage_operands(verb: &str, raw: &str) -> Option<Result<(), String>> {
    if is_language_stage_verb(verb) {
        return None;
    }
    let registry = StageRegistry::default();
    let canonical = registry
        .canonical_name(verb)
        .map(|s| s.to_string())
        .unwrap_or_else(|| verb.to_ascii_uppercase());
    let args = operands_for_stage(verb, raw);
    match canonical.as_str() {
        "LITERAL" | "STRLITERAL" => Some(Ok(())),
        "LOCATE" | "NLOCATE" | "NOTLOCATE" => Some(check_locate(&args)),
        "TAKE" | "DROP" => Some(check_ends(&args)),
        "CHANGE" => Some(check_change(&args)),
        "COUNT" => Some(check_count(&args)),
        "CONSOLE" | "TERMINAL" => Some(check_console(&args)),
        "SPEC" | "SPECS" => Some(check_spec(&args)),
        name if FILE_VERBS.contains(&name) => Some(file_path(&args, name).map(|_| ())),
        _ => None,
    }
}

fn check_console(args: &[String]) -> Result<(), String> {
    if args.is_empty() {
        return Ok(());
    }
    check_console_read(args).map_err(rewrite_console_error)
}

fn rewrite_console_error(msg: String) -> String {
    msg.replace(
        " is CMS-specific and is not implemented",
        " is not implemented",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(verb: &str, raw: &str) {
        match check_stage_operands(verb, raw) {
            Some(Ok(())) => {}
            other => panic!("{verb} {raw:?} expected Ok, got {other:?}"),
        }
    }

    fn err_contains(verb: &str, raw: &str, needle: &str) {
        match check_stage_operands(verb, raw) {
            Some(Err(msg)) => assert!(
                msg.to_ascii_lowercase()
                    .contains(&needle.to_ascii_lowercase()),
                "{verb} {raw:?} error {msg:?} missing {needle:?}"
            ),
            other => panic!("{verb} {raw:?} expected Err({needle}), got {other:?}"),
        }
    }

    #[test]
    fn first_batch_accepts_valid_operands() {
        ok("LITERAL", "hello world");
        ok("literal", "");
        ok("LOCATE", "/ap/");
        ok("NLOCATE", "ANYCASE /x/");
        ok("NOTLOCATE", "");
        ok("TAKE", "2");
        ok("DROP", "LAST 1");
        ok("CHANGE", "/a/b/");
        ok("COUNT", "LINES");
        ok("CONSOLE", "");
        ok("CONSOLE", "NOEOF");
        ok("TERMINAL", "EOF /./");
        ok("SPEC", "1-* 1");
        ok("SPECS", "1-* 1");
        ok("<", "notes.txt");
        ok("DISK", "C:\\temp\\a.txt");
        ok(">", "out.txt");
        ok(">>", "log.txt");
        ok("FILEFAST", "data.csv");
    }

    #[test]
    fn first_batch_rejects_bad_operands() {
        err_contains("LOCATE", "/ap/ extra", "unexpected");
        err_contains("TAKE", "BYTES", "not implemented");
        err_contains("DROP", "nope", "unrecognized");
        err_contains("CHANGE", "", "change string");
        err_contains("COUNT", "", "requires at least one");
        err_contains("COUNT", "BANANAS", "unrecognized");
        err_contains("CONSOLE", "ZOOM", "unrecognized");
        err_contains("SPEC", "", "specification");
        err_contains("<", "", "path is required");
        err_contains("DISK", "", "path is required");
        err_contains(">", "", "path is required");
    }

    #[test]
    fn console_unimplemented_keyword_does_not_mention_cms() {
        match check_stage_operands("CONSOLE", "ASYNCHRONOUSLY") {
            Some(Err(msg)) => {
                let low = msg.to_ascii_lowercase();
                assert!(low.contains("not implemented"), "{msg}");
                assert!(!low.contains("cms"), "{msg}");
                assert!(!low.contains("ibm"), "{msg}");
            }
            other => panic!("expected error, got {other:?}"),
        }
    }

    #[test]
    fn language_stages_are_never_checked() {
        assert!(check_stage_operands("upper.py", "owner").is_none());
        assert!(check_stage_operands("script.rexx", "arg").is_none());
        assert!(check_stage_operands("REXX", "arg").is_none());
        assert!(check_stage_operands("thing.rex", "").is_none());
    }

    #[test]
    fn stages_without_a_checker_are_silent() {
        assert!(check_stage_operands("SORT", "1").is_none());
        assert!(check_stage_operands("HOLE", "").is_none());
        assert!(check_stage_operands("BUFFER", "nope").is_none());
        assert!(check_stage_operands("NOTASTAGE", "/x/").is_none());
    }

    #[test]
    fn abbreviations_use_the_canonical_checker() {
        ok("CONS", "NOEOF");
        err_contains("locate", "/ap/ extra", "unexpected");
        ok("SPECS", "1-* 1");
    }
}
