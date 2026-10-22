"""Bootstrap for `python -u -m pipelines.stage -- script.py [args…]`."""

from __future__ import annotations

import runpy
import sys
import traceback

from . import _protocol as proto
from . import bind


MIN_VERSION = (3, 10)


def _version_tuple() -> tuple[int, int, int]:
    v = sys.version_info
    return (v.major, v.minor, v.micro)


def format_old_python_error(found: tuple[int, int, int]) -> str:
    return (
        "Python 3.10 or later is required; found "
        f"{found[0]}.{found[1]}.{found[2]}"
    )


def version_meets_min(found: tuple[int, int, int]) -> bool:
    return found >= MIN_VERSION


def main(argv: list[str] | None = None) -> int:
    found = _version_tuple()
    proto_in = None
    proto_out = None
    try:
        proto_in, proto_out = proto.bind_user_stdio()
        conn = proto.Connection(proto_in, proto_out)
        if not version_meets_min(found):
            conn.send_fail(format_old_python_error(found))
            return 1

        typ, payload = proto.read_frame(proto_in)
        if typ != proto.HELLO:
            conn.send_fail(f"expected HELLO, got type 0x{typ:02x}")
            return 1
        _version, script, stage_args = proto.decode_hello(payload)
        proto.write_frame(proto_out, proto.HELLO_OK)

        if argv is None:
            argv = sys.argv
        # sys.argv[0] is the script path; remainder are stage args.
        sys.argv = [script, *stage_args]
        bind(conn, stage_args, script)

        try:
            runpy.run_path(script, run_name="__main__")
        except SystemExit as e:
            code = e.code
            if code is None:
                rc = 0
            elif isinstance(code, int):
                rc = code
            else:
                rc = 1
            conn.send_exit(rc)
            return rc
        except Exception:
            conn.send_fail(traceback.format_exc())
            return 1
        conn.send_exit(0)
        return 0
    except proto.ProtocolError as e:
        if proto_out is not None:
            try:
                proto.write_frame(proto_out, proto.FAIL, str(e).encode("utf-8", "replace"))
            except Exception:
                pass
        return 1
    finally:
        if proto_in is not None:
            try:
                proto_in.close()
            except Exception:
                pass
        if proto_out is not None:
            try:
                proto_out.close()
            except Exception:
                pass


if __name__ == "__main__":
    raise SystemExit(main())
