"""Framed binary protocol used between pipe and a Python stage process."""

from __future__ import annotations

import os
import struct
import sys
from typing import BinaryIO

MAX_PAYLOAD = 16 * 1024 * 1024

HELLO = 0x01
READTO = 0x10
PEEKTO = 0x11
OUTPUT = 0x12
SHORT = 0x13
EXIT = 0x14
FAIL = 0x15

HELLO_OK = 0x80
RECORD = 0x81
EOF = 0x82
OK = 0x83
ERR = 0x84


class ProtocolError(Exception):
    pass


def write_frame(out: BinaryIO, typ: int, payload: bytes = b"") -> None:
    if len(payload) > MAX_PAYLOAD:
        raise ProtocolError("payload exceeds 16 MiB")
    out.write(struct.pack("<I", len(payload)))
    out.write(bytes((typ,)))
    if payload:
        out.write(payload)
    out.flush()


def read_frame(inp: BinaryIO) -> tuple[int, bytes]:
    header = _read_exact(inp, 4)
    (length,) = struct.unpack("<I", header)
    if length > MAX_PAYLOAD:
        raise ProtocolError("payload exceeds 16 MiB")
    typ = _read_exact(inp, 1)[0]
    payload = _read_exact(inp, length) if length else b""
    return typ, payload


def _read_exact(inp: BinaryIO, n: int) -> bytes:
    if n == 0:
        return b""
    buf = bytearray()
    while len(buf) < n:
        chunk = inp.read(n - len(buf))
        if not chunk:
            raise ProtocolError("unexpected end of protocol stream")
        buf.extend(chunk)
    return bytes(buf)


def decode_hello(payload: bytes) -> tuple[int, str, list[str]]:
    if len(payload) < 10:
        raise ProtocolError("hello payload is truncated")
    version, flags, path_len = struct.unpack_from("<HII", payload, 0)
    if version != 1:
        raise ProtocolError(f"unsupported protocol version {version}")
    off = 10
    path = payload[off : off + path_len].decode("utf-8")
    off += path_len
    if off + 4 > len(payload):
        raise ProtocolError("hello argc is truncated")
    (argc,) = struct.unpack_from("<I", payload, off)
    off += 4
    args: list[str] = []
    for _ in range(argc):
        if off + 4 > len(payload):
            raise ProtocolError("hello arg length is truncated")
        (n,) = struct.unpack_from("<I", payload, off)
        off += 4
        args.append(payload[off : off + n].decode("utf-8"))
        off += n
    del flags
    return version, path, args


def decode_err(payload: bytes) -> tuple[int, str]:
    if len(payload) < 4:
        raise ProtocolError("ERR payload is truncated")
    (rc,) = struct.unpack_from("<i", payload, 0)
    msg = payload[4:].decode("utf-8", "replace")
    return rc, msg


def encode_exit(code: int) -> bytes:
    return struct.pack("<i", code)


def encode_io(stream: int = 0, data: bytes = b"") -> bytes:
    """I/O request/record payload: u32le stream number plus optional data."""
    return struct.pack("<I", int(stream)) + data


def decode_io(payload: bytes) -> tuple[int, bytes]:
    if len(payload) < 4:
        return 0, payload
    (stream,) = struct.unpack_from("<I", payload, 0)
    return stream, payload[4:]


def decode_ok(payload: bytes) -> int:
    # Stream-prefixed OK is 8 bytes (u32le stream, i32le rc). Bare i32 remains
    # accepted so a 4-byte reply still decodes.
    if len(payload) >= 8:
        (rc,) = struct.unpack_from("<i", payload, 4)
        return rc
    if len(payload) < 4:
        raise ProtocolError("OK payload is truncated")
    (rc,) = struct.unpack_from("<i", payload, 0)
    return rc


class Connection:
    def __init__(self, inp: BinaryIO, out: BinaryIO) -> None:
        self._in = inp
        self._out = out
        self.exited = False

    def request(self, typ: int, payload: bytes = b"") -> tuple[int, bytes]:
        write_frame(self._out, typ, payload)
        return read_frame(self._in)

    def send_exit(self, code: int) -> None:
        if self.exited:
            return
        write_frame(self._out, EXIT, encode_exit(code))
        self.exited = True

    def send_fail(self, message: str) -> None:
        if self.exited:
            return
        write_frame(self._out, FAIL, message.encode("utf-8", "replace"))
        self.exited = True


def _set_non_inheritable(fd: int) -> None:
    if os.name == "nt":
        import ctypes
        import msvcrt

        kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
        handle_flag_inherit = 0x00000001
        handle = msvcrt.get_osfhandle(fd)
        if not kernel32.SetHandleInformation(handle, handle_flag_inherit, 0):
            err = ctypes.get_last_error()
            raise OSError(err, "SetHandleInformation failed")
    else:
        import fcntl

        flags = fcntl.fcntl(fd, fcntl.F_GETFD)
        fcntl.fcntl(fd, fcntl.F_SETFD, flags | fcntl.FD_CLOEXEC)


def _reset_windows_std_handles() -> None:
    if os.name != "nt":
        return
    import ctypes
    import msvcrt

    kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
    std_input_handle = ctypes.c_ulong(-10 & 0xFFFFFFFF)
    std_output_handle = ctypes.c_ulong(-11 & 0xFFFFFFFF)
    kernel32.SetStdHandle(std_input_handle, msvcrt.get_osfhandle(0))
    kernel32.SetStdHandle(std_output_handle, msvcrt.get_osfhandle(1))


def bind_user_stdio() -> tuple[BinaryIO, BinaryIO]:
    """Move the protocol off fd 0/1 so print() cannot corrupt frames."""
    proto_in_fd = os.dup(0)
    proto_out_fd = os.dup(1)
    _set_non_inheritable(proto_in_fd)
    _set_non_inheritable(proto_out_fd)

    os.dup2(2, 1)
    null_fd = os.open(os.devnull, os.O_RDONLY)
    try:
        os.dup2(null_fd, 0)
    finally:
        os.close(null_fd)

    _reset_windows_std_handles()

    sys.stdin = open(0, "r", encoding="utf-8", errors="replace", closefd=False)
    sys.stdout = open(1, "w", encoding="utf-8", errors="replace", closefd=False)
    try:
        sys.stderr.reconfigure(line_buffering=True)
    except Exception:
        pass

    proto_in = os.fdopen(proto_in_fd, "rb", buffering=0)
    proto_out = os.fdopen(proto_out_fd, "wb", buffering=0)
    return proto_in, proto_out
