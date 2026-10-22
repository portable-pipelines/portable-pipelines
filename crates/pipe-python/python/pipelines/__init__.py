"""Host-facing pipeline commands for Python stages (IBM names, lowercase)."""

from __future__ import annotations

from typing import Iterator

from . import _protocol as proto

rc: int = 0
args: list[str] = []
arg: str = ""

_conn: proto.Connection | None = None


class PipelineError(Exception):
    """Raised when the host replies ERR to a pipeline command."""


def _require_conn() -> proto.Connection:
    if _conn is None:
        raise RuntimeError("pipelines is not connected to a pipe host")
    return _conn


def _apply_reply(typ: int, payload: bytes) -> bytes | None:
    global rc
    if typ == proto.RECORD:
        rc = 0
        return payload
    if typ == proto.EOF:
        rc = 12
        return None
    if typ == proto.OK:
        rc = proto.decode_ok(payload)
        return None
    if typ == proto.ERR:
        rc, msg = proto.decode_err(payload)
        raise PipelineError(msg or f"pipeline command failed rc={rc}")
    raise PipelineError(f"unexpected host reply type 0x{typ:02x}")


def readto() -> bytes | None:
    typ, payload = _require_conn().request(proto.READTO)
    return _apply_reply(typ, payload)


def peekto() -> bytes | None:
    typ, payload = _require_conn().request(proto.PEEKTO)
    return _apply_reply(typ, payload)


def output(data: bytes | str) -> None:
    if isinstance(data, str):
        payload = data.encode("utf-8", "surrogateescape")
    elif isinstance(data, (bytes, bytearray, memoryview)):
        payload = bytes(data)
    else:
        raise TypeError("output() accepts bytes or str")
    typ, reply = _require_conn().request(proto.OUTPUT, payload)
    _apply_reply(typ, reply)


def short() -> None:
    typ, payload = _require_conn().request(proto.SHORT)
    _apply_reply(typ, payload)


def records() -> Iterator[bytes]:
    while True:
        rec = readto()
        if rec is None:
            break
        yield rec


def exit(code: int = 0) -> None:
    _require_conn().send_exit(int(code))
    raise SystemExit(int(code))


def bind(conn: proto.Connection, stage_args: list[str], script: str) -> None:
    global _conn, args, arg, rc
    _conn = conn
    args = list(stage_args)
    arg = " ".join(stage_args)
    rc = 0
    del script
