# Benchmark for `json objects`

This folder holds a **representative** Python stage and a driver you can run
against a release build. It is not John's `json_objects.py`. If that file is
not in this tree, this reconstruction is the comparison baseline.

From the repository root, after `cargo build --release -p pipe-cli`:

```bash
python3 scripts/bench-json-objects/run_bench.py
```

The driver writes a temp file of consecutive compact JSON objects (not a
top-level array), warms each command once, then reports median wall-clock
time for:

- native and Python startup (one small object)
- native and Python throughput (the whole file to `hole`)
- native and Python `take 10`

Override `--pipe`, `--objects`, `--payload-bytes`, `--runs`, and `--data` as
needed. The printed report names the binary, host, data size, and the
limitations of the comparison.
