//! Python-language stage host for Portable Pipelines (non-IBM extension).
//!
//! `.py` verbs are resolved by `pipe-stages`; the CLI installs this hook so
//! each stage is an out-of-process interpreter talking a framed protocol.

mod discover;
mod host;
mod protocol;

use std::rc::Rc;

use pipe_core::{PipeError, PipeResult, StageCtx};

pub use discover::{
    crate_python_dir, discovery_hint, find_pipelines_parent, find_python, format_old_python_error,
    parse_version_line, pipe_require_python, python_available, query_python_version, PythonVersion,
};
pub use host::run_python_stage;

/// Install the Python stage hook into `pipe-stages` so `.py` verbs resolve.
pub fn install_hook() {
    let hook: pipe_stages::PythonHook =
        Rc::new(|ctx, verb, args| run_python_stage(ctx, verb, args).map_err(|e| e.to_string()));
    pipe_stages::set_python_hook(hook);
}

/// Resolve a script path relative to CWD (same rule as REXX filters).
pub fn find_script(name: &str) -> Option<std::path::PathBuf> {
    let p = std::path::Path::new(name);
    if p.exists() {
        Some(p.to_path_buf())
    } else {
        None
    }
}

pub fn run_python_file(ctx: &mut StageCtx, path: &str, args: &[String]) -> PipeResult<()> {
    if find_script(path).is_none() {
        return Err(PipeError::io(format!("Python script not found: {path}")));
    }
    run_python_stage(ctx, path, args)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::path::{Path, PathBuf};
    use std::rc::Rc;

    use pipe_core::{PipelineBuilder, Record};
    use pipe_stages::run_spec_with_return_code;

    fn repo_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    fn fixture(name: &str) -> PathBuf {
        repo_root().join("tests/pipelines/python").join(name)
    }

    fn example(name: &str) -> PathBuf {
        repo_root().join("examples").join(name)
    }

    fn python_env_lock() -> std::sync::MutexGuard<'static, ()> {
        crate::discover::env_lock()
    }

    /// Hold the env lock for the whole test so `PIPE_PYTHON` cannot leak.
    fn skip_or_lock() -> Option<std::sync::MutexGuard<'static, ()>> {
        let guard = python_env_lock();
        if python_available() {
            return Some(guard);
        }
        drop(guard);
        if pipe_require_python() {
            panic!(
                "PIPE_REQUIRE_PYTHON=1 but Python 3.10+ was not discovered ({})",
                discovery_hint()
            );
        }
        None
    }

    fn run_py(script: &Path, args: &[&str], input: &[&[u8]]) -> Result<Vec<Vec<u8>>, String> {
        install_hook();
        let collected = Rc::new(RefCell::new(Vec::new()));
        let out = Rc::clone(&collected);
        let script = script.to_path_buf();
        let args: Vec<String> = args.iter().map(|s| (*s).to_string()).collect();
        let incoming: Vec<Vec<u8>> = input.iter().map(|s| s.to_vec()).collect();

        let mut b = PipelineBuilder::new();
        let src = b.add_stage(
            "src",
            Box::new(move |ctx| {
                for rec in incoming {
                    if ctx.write(Record::from(rec)).is_err() {
                        break;
                    }
                }
            }),
        );
        let py = b.add_stage(
            "py",
            Box::new(move |ctx| {
                if let Err(e) = run_python_stage(ctx, script.to_str().unwrap(), &args) {
                    if pipe_core::has_stage_context(&e.to_string()) {
                        ctx.fail(e.to_string());
                    } else {
                        ctx.fail(format!("Python stage: {e}"));
                    }
                }
            }),
        );
        let sink = b.add_stage(
            "sink",
            Box::new(move |ctx| {
                while let Ok(Some(rec)) = ctx.read() {
                    out.borrow_mut().push(rec.as_bytes().to_vec());
                }
            }),
        );
        b.connect_primary(src, py);
        b.connect_primary(py, sink);
        b.run().map_err(|e| e.to_string())?;
        let result = collected.borrow().clone();
        Ok(result)
    }

    #[test]
    fn pipe_require_python_finds_interpreter() {
        let _guard = python_env_lock();
        if !pipe_require_python() {
            return;
        }
        assert!(
            python_available(),
            "PIPE_REQUIRE_PYTHON=1 but Python 3.10+ was not discovered ({})",
            discovery_hint()
        );
        let py = find_python().expect("interpreter");
        let v = query_python_version(&py).expect("version");
        assert!(v.meets_min(), "found {}", v.display());
    }

    #[test]
    fn upper_filter_bytes() {
        let _guard = match skip_or_lock() {
            Some(g) => g,
            None => return,
        };
        let out = run_py(&example("upper.py"), &[], &[b"hello", b"World"]).unwrap();
        assert_eq!(out, vec![b"HELLO".to_vec(), b"WORLD".to_vec()]);
    }

    #[test]
    fn records_iterator_and_emit() {
        let _guard = match skip_or_lock() {
            Some(g) => g,
            None => return,
        };
        let out = run_py(&example("emit.py"), &["a", "b", "c"], &[]).unwrap();
        assert_eq!(out, vec![b"a".to_vec(), b"b".to_vec(), b"c".to_vec()]);
    }

    #[test]
    fn peekto_does_not_drop_or_duplicate() {
        let _guard = match skip_or_lock() {
            Some(g) => g,
            None => return,
        };
        let out = run_py(&fixture("bracket.py"), &[], &[b"hi", b"there"]).unwrap();
        assert_eq!(out, vec![b"[hi]".to_vec(), b"[there]".to_vec()]);
    }

    #[test]
    fn output_str_encodes_non_ascii() {
        let _guard = match skip_or_lock() {
            Some(g) => g,
            None => return,
        };
        let out = run_py(&fixture("str_output.py"), &[], &[b"x"]).unwrap();
        assert_eq!(out, vec!["café".as_bytes().to_vec()]);
    }

    #[test]
    fn print_does_not_corrupt_records() {
        let _guard = match skip_or_lock() {
            Some(g) => g,
            None => return,
        };
        let out = run_py(&fixture("print_pass.py"), &[], &[b"keep"]).unwrap();
        assert_eq!(out, vec![b"keep".to_vec()]);
        assert!(!out.iter().any(|r| r.windows(5).any(|w| w == b"debug")));
    }

    fn with_repo_cwd<T>(f: impl FnOnce() -> T) -> T {
        let prev = std::env::current_dir().expect("cwd");
        std::env::set_current_dir(repo_root()).expect("chdir repo root");
        let result = f();
        let _ = std::env::set_current_dir(prev);
        result
    }

    #[test]
    fn exit_n_matches_rexx() {
        let _guard = match skip_or_lock() {
            Some(g) => g,
            None => return,
        };
        install_hook();
        pipe_rexx::install_hook();
        let (py, rexx) = with_repo_cwd(|| {
            let py =
                run_spec_with_return_code("literal x | tests/pipelines/python/exit7.py | hole")
                    .expect("python exit");
            let rexx =
                run_spec_with_return_code("literal x | tests/pipelines/python/exit7.rexx | hole")
                    .expect("rexx exit");
            (py, rexx)
        });
        assert_eq!(py, rexx, "Python {py} vs REXX {rexx}");
        assert_eq!(py, 7);
    }

    #[test]
    fn two_python_stages_same_file() {
        let _guard = match skip_or_lock() {
            Some(g) => g,
            None => return,
        };
        let out = run_py(&example("upper.py"), &[], &[b"hello"]).unwrap();
        assert_eq!(out, vec![b"HELLO".to_vec()]);
        install_hook();
        let rc = with_repo_cwd(|| {
            run_spec_with_return_code(
                "literal hello | examples/upper.py | examples/upper.py | hole",
            )
            .expect("two stages")
        });
        assert_eq!(rc, 0);
    }

    #[test]
    fn endchar_multipipe_single_stream() {
        let _guard = match skip_or_lock() {
            Some(g) => g,
            None => return,
        };
        install_hook();
        let rc = with_repo_cwd(|| {
            run_spec_with_return_code(
                "(ENDCHAR ?) examples/emit.py a b | examples/upper.py | hole ? literal z | hole",
            )
            .expect("endchar")
        });
        assert_eq!(rc, 0);
    }

    #[test]
    fn secondary_label_is_rejected() {
        let _guard = match skip_or_lock() {
            Some(g) => g,
            None => return,
        };
        install_hook();
        let err = with_repo_cwd(|| {
            run_spec_with_return_code(
                "(ENDCHAR ?) literal hello | T: examples/upper.py | hole ? T: | hole",
            )
            .expect_err("secondary must fail")
        });
        let msg = err.to_string();
        assert!(
            msg.contains("single-stream") || msg.contains("secondary stream"),
            "{msg}"
        );
        assert!(
            msg.contains("... Issued from stage 2 of pipeline 1"),
            "{msg}"
        );
    }

    #[test]
    fn syntax_error_is_a_stage_error() {
        let _guard = match skip_or_lock() {
            Some(g) => g,
            None => return,
        };
        let err = run_py(&fixture("syntax_bad.py"), &[], &[b"x"]).unwrap_err();
        assert!(
            err.contains("SyntaxError") || err.contains("invalid syntax") || err.contains("syntax"),
            "{err}"
        );
    }

    #[test]
    fn exception_is_a_stage_error() {
        let _guard = match skip_or_lock() {
            Some(g) => g,
            None => return,
        };
        let err = run_py(&fixture("boom.py"), &[], &[b"x"]).unwrap_err();
        assert!(
            err.contains("intentional") || err.contains("RuntimeError"),
            "{err}"
        );
    }

    #[test]
    fn old_python_via_pipe_python_env() {
        let _guard = python_env_lock();
        let fake = std::env::temp_dir().join(format!(
            "cms-pipelines-fake-python39-{}",
            std::process::id()
        ));
        std::fs::write(&fake, "#!/bin/sh\necho 3.9.18\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perm = std::fs::metadata(&fake).unwrap().permissions();
            perm.set_mode(0o755);
            std::fs::set_permissions(&fake, perm).unwrap();
        }
        let prev = std::env::var_os("PIPE_PYTHON");
        std::env::set_var("PIPE_PYTHON", &fake);
        let py = find_python().expect("fake interpreter");
        let v = query_python_version(&py).expect("fake version");
        assert!(!v.meets_min(), "{}", v.display());
        let msg = format_old_python_error(v);
        assert!(msg.contains("3.9.18"), "{msg}");
        match prev {
            Some(p) => std::env::set_var("PIPE_PYTHON", p),
            None => std::env::remove_var("PIPE_PYTHON"),
        }
        let _ = std::fs::remove_file(&fake);
    }
}
