# Compat fixtures

Each `*.txt` file documents a pipeline specification and expected records:

```text
# spec
literal hello
# expect
hello
```

Executable coverage lives in crate integration tests (`pipe-stages/tests`, `pipe-cli/tests`).
Add fixtures here as you widen IBM behavioral parity.
