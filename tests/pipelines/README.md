# Pipeline fixtures

Record-oriented cases: a PIPE specification plus the records (or error) it
must produce. The runner lives in `crates/pipe-cli/tests/pipeline_fixtures.rs`
and is part of `cargo test --workspace`.

`examples/README.md` `pipe "…"` lines are harvested into
`readme-harvest.pipefix`. A completeness test fails if a README `pipe` line
is missing from that file.

## Format

```text
%% case-id
literal abc | spec 1-* 1 /*/ next | console
=> abc*

%% two-streams
(ENDCHAR ?) examples/emit.rexx a b | T: take | console ? T: | console
=> primary: a
=> secondary: b

%% rejected
literal keep | spec | hole
=> error: one or more items

%% not-asserted
literal hi | > /tmp/out.txt
skip: writes a host file
```

- `%% id` starts a case. The following lines are the specification until a
  `=>` or `skip:` line.
- `=> a / b / c` is the primary-output record list. Split on space-slash-space.
  `␠` is a space. `"quoted"` keeps interior blanks and slashes.
- `(empty)` means no records (README shows `Output: *(no records)*`).
- `=> primary:` / `=> secondary:` when the spec has two `console` stages
  (typical `(ENDCHAR ?)` label pair).
- `=> error: substring` expects a failed run whose message contains that text.
  Repeat the line to require several substrings (primary text plus CMS
  `... Issued from stage` / `... Running` context).
- `skip: reason` keeps the spec in the harvest without running it.
- `need: python3` runs the case only when Python 3.10+ is discovered. Otherwise the case is skipped, unless `PIPE_REQUIRE_PYTHON=1` (then it fails).

The runner replaces each `CONSOLE` / `TERMINAL` with an in-memory collector
(`>>COLLECT`, or `>>COLLECT_P` / `>>COLLECT_S`). It does not spawn the `pipe`
binary and does not read the terminal. Work directory is the repository root
so `examples/…` and `< file` paths resolve. REXX stages use
`pipe_rexx::install_hook()`. Python stages use `pipe_python::install_hook()`.

A space before `|` is a trailing blank in LITERAL’s argument (1.1.11 p.412).
README `pipe` lines that are not demonstrating those blanks use `literal abc|`
with no space so the printed output records match.
