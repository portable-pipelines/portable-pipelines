# Contributing

Thank you for wanting to help with Portable Pipelines. This page is for people building and testing from a clone.

## Using Cursor (first time)

1. Open your clone in Cursor.
2. Ask the Agent to run tests: `cargo test`.
3. Change a stage under `crates/pipe-stages`, re-run tests, and try `cargo run -p pipe-cli -- "…"`.

Cursor Cloud VMs install Regina and Python from `.cursor/environment.json`. That file cannot set environment variables, so export the two flags below in the shell before `cargo test` when you want a missing install to fail instead of falling back or skipping.

## Tests

From the repo root:

```bash
cargo test --workspace
```

On Windows PowerShell:

```powershell
cargo test --workspace
```

`rust-toolchain.toml` pins the `stable` channel, so you do not need `cargo +stable`.

### Requiring Regina and Python

REXX stages prefer a user-installed Regina library and otherwise use the embedded subset interpreter. Python stages need Python 3.10 or later; without it, those cases skip.

- `PIPE_REQUIRE_REGINA=1` makes tests fail if Regina is not discovered or loaded, instead of silently using the subset interpreter.
- `PIPE_REQUIRE_PYTHON=1` makes tests fail if Python 3.10+ is not discovered, instead of skipping.

Contributors who have both installed (and Cursor Cloud agents) should export them so a missing install cannot hide a real bug:

```bash
PIPE_REQUIRE_REGINA=1 PIPE_REQUIRE_PYTHON=1 cargo test --workspace
```

If you do not have Regina or Python, leave the matching variable unset so the suite still passes on the subset path or by skipping those cases.

Install notes are in the [README](README.md).

## Reference PDF

Behavioral authority is CMS/TSO Pipelines Author's Edition 1.1.11 (document SL26-0018-05). Keep your local copy of that PDF; it is not included in this repo (IBM copyright). Do not commit it. See [docs/reference/README.md](docs/reference/README.md).
