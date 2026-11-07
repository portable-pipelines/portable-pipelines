"""Representative Python stage for consecutive JSON objects.

This is NOT John's json_objects.py. It is a reconstructed baseline used to
compare a release-build native `json objects` stage with a Python stage that
does the same standard-JSON split. It parses with json.JSONDecoder.raw_decode,
emits compact objects, and stops when OUTPUT returns RC 12 (downstream TAKE
closed the stream). It does not rewrite MongoDB shell constructors.
"""

from __future__ import annotations

import json

import pipelines as p

_DECODER = json.JSONDecoder()
_MONGO = ("ObjectId(", "ISODate(", "NumberLong(", "NumberInt(")


def _diagnose(text: str) -> str:
    if any(marker in text for marker in _MONGO):
        return (
            "this is not standard JSON. MongoDB shell constructors such as "
            "ObjectId(...), ISODate(...), NumberLong(...), and NumberInt(...) "
            "are not JSON. This representative Python stage does not convert "
            "them. Export the data as JSON, or as Extended JSON "
            '(for example {"$oid":"..."}).'
        )
    stripped = text.lstrip()
    if stripped.startswith("["):
        return (
            "input is not a JSON object. This stage reads consecutive JSON "
            "objects, not a top-level array."
        )
    return "input is not a JSON object. This stage reads only JSON objects."


def _emit(obj: object) -> None:
    if not isinstance(obj, dict):
        raise ValueError(
            "input is not a JSON object. This stage reads only JSON objects."
        )
    p.output(json.dumps(obj, separators=(",", ":"), ensure_ascii=False))
    if p.rc == 12:
        p.short()
        p.exit(0)


def main() -> None:
    buf = ""
    while True:
        rec = p.readto()
        if rec is None:
            break
        chunk = rec.decode("utf-8")
        if buf:
            buf += "\n" + chunk
        else:
            buf = chunk
        while True:
            s = buf.lstrip()
            if not s:
                buf = ""
                break
            try:
                obj, idx = _DECODER.raw_decode(s)
            except json.JSONDecodeError:
                break
            _emit(obj)
            buf = s[idx:]
    leftover = buf.strip()
    if leftover:
        if any(marker in leftover for marker in _MONGO):
            raise ValueError(_diagnose(leftover))
        if leftover.startswith("{"):
            raise ValueError("unclosed JSON object")
        raise ValueError(_diagnose(leftover))


if __name__ == "__main__":
    main()
