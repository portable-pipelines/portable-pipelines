#!/usr/bin/env python3
"""Reproducible timings for native `json objects` versus a representative Python stage.

This script is the reviewer's copy-paste way to repeat the comparison. It is
not John's json_objects.py, and it does not claim a general speedup factor.

It writes a data file of consecutive compact JSON objects, then times:

- startup: one small object through the full pipeline
- throughput: the whole generated file to hole
- early take: take 10 of that same file

Native and Python runs use the same data file and the same release `pipe`
binary. Python startup includes interpreter launch per stage, which is why
startup is reported separately from throughput.
"""

from __future__ import annotations

import argparse
import json
import os
import platform
import statistics
import subprocess
import sys
import time
from pathlib import Path


def repo_root() -> Path:
    return Path(__file__).resolve().parents[2]


def write_data(path: Path, n: int, payload: str) -> int:
    # Consecutive objects, no wrapping array, no JSON Lines requirement.
    with path.open("w", encoding="utf-8", newline="\n") as f:
        for i in range(n):
            rec = {"n": i, "title": f"item-{i:05d}", "note": payload}
            f.write(json.dumps(rec, separators=(",", ":")))
            f.write("\n")
    return path.stat().st_size


def run_once(cmd: list[str], cwd: Path, extra_env: dict[str, str] | None = None) -> float:
    env = os.environ.copy()
    if extra_env:
        env.update(extra_env)
    start = time.perf_counter()
    proc = subprocess.run(
        cmd,
        cwd=cwd,
        env=env,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.PIPE,
        check=False,
    )
    elapsed = time.perf_counter() - start
    if proc.returncode != 0:
        err = proc.stderr.decode("utf-8", "replace")
        raise RuntimeError(f"command failed rc={proc.returncode}: {' '.join(cmd)}\n{err}")
    return elapsed


def median_runs(cmd: list[str], cwd: Path, runs: int, extra_env: dict[str, str] | None = None) -> list[float]:
    times = []
    for _ in range(runs):
        times.append(run_once(cmd, cwd, extra_env))
    return times


def fmt(times: list[float]) -> str:
    med = statistics.median(times)
    return f"median {med:.4f}s (min {min(times):.4f}s, max {max(times):.4f}s, n={len(times)})"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--pipe",
        default=str(repo_root() / "target" / "release" / "pipe"),
        help="Path to a release-build pipe binary",
    )
    parser.add_argument("--objects", type=int, default=50_000)
    parser.add_argument("--payload-bytes", type=int, default=64)
    parser.add_argument("--runs", type=int, default=5)
    parser.add_argument(
        "--data",
        default=str(Path("/tmp") / "pipe-json-objects-bench.json"),
    )
    args = parser.parse_args()

    root = repo_root()
    pipe = Path(args.pipe)
    if not pipe.is_file():
        raise SystemExit(f"pipe binary not found: {pipe} (build with cargo build --release -p pipe-cli)")

    # Relative to the repo root so the pipe parser does not treat a leading
    # `/` as a delimited string.
    py_stage = Path("scripts/bench-json-objects/json_objects_representative.py")
    data = Path(args.data)
    payload = "x" * args.payload_bytes
    size = write_data(data, args.objects, payload)

    native_all = f"< {data} | json objects | hole"
    native_take = f"< {data} | json objects | take 10 | hole"
    native_one = 'literal {"n":0,"title":"item"} | json objects | hole'
    py_all = f"< {data} | {py_stage} | hole"
    py_take = f"< {data} | {py_stage} | take 10 | hole"
    py_one = f'literal {{"n":0,"title":"item"}} | {py_stage} | hole'

    print("portable-pipelines json objects benchmark")
    print(f"pipe:     {pipe}")
    print(f"python:   {sys.executable} {sys.version.split()[0]}")
    print(f"host:     {platform.system()} {platform.release()} {platform.machine()}")
    print(f"data:     {data} ({size} bytes, {args.objects} objects, payload {args.payload_bytes} bytes)")
    print(f"runs:     {args.runs} timed runs after one warmup each")
    print("python stage: scripts/bench-json-objects/json_objects_representative.py")
    print("  (reconstructed representative; not John's json_objects.py)")
    print()

    cases = [
        ("native startup (1 object)", [str(pipe), native_one]),
        ("python startup (1 object)", [str(pipe), py_one]),
        ("native throughput (all)", [str(pipe), native_all]),
        ("python throughput (all)", [str(pipe), py_all]),
        ("native take 10", [str(pipe), native_take]),
        ("python take 10", [str(pipe), py_take]),
    ]
    for label, cmd in cases:
        run_once(cmd, root)  # warmup
        times = median_runs(cmd, root, args.runs)
        print(f"{label}: {fmt(times)}")

    print()
    print("Limitations:")
    print("- Wall-clock only. No claim that native is a fixed multiple of Python.")
    print("- Python pays per-stage interpreter startup; that dominates the 1-object case.")
    print("- take 10 still reads enough lines from `<` to complete ten objects.")
    print("- The Python stage is a representative reconstruction, not John's file.")
    print("- Output goes to hole, so console I/O is not in the numbers.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
