"""Write GitHub Actions job outputs without interpolation hazards."""

from __future__ import annotations

import re
from pathlib import Path

from errors import ReleaseError

KEY_RE = re.compile(r"^[a-z][a-z0-9_]*$")
SAFE_VALUE_RE = re.compile(r"^[A-Za-z0-9._/-]*$")


def write_github_output(path: Path, values: dict[str, str]) -> None:
    lines: list[str] = []
    for key, value in values.items():
        if not KEY_RE.fullmatch(key):
            raise ReleaseError(f"unsafe GitHub output name: {key!r}")
        if not isinstance(value, str):
            raise ReleaseError(f"GitHub output {key} must be a string")
        if "\n" in value or "\r" in value:
            raise ReleaseError(f"GitHub output {key} must be a single line")
        if not SAFE_VALUE_RE.fullmatch(value):
            raise ReleaseError(
                f"GitHub output {key} has characters that are not allowed: {value!r}"
            )
        lines.append(f"{key}={value}")
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("a", encoding="utf-8") as handle:
        handle.write("\n".join(lines) + "\n")
