"""Python SDK for compact-core.

The binding uses Python's standard-library ``ctypes`` module and the stable C
ABI exposed by ``compact-ffi``. Returned Rust buffers are copied into Python
``bytes`` before the Rust allocation is released.
"""

from __future__ import annotations

import ctypes
import os
from pathlib import Path


class CompactError(RuntimeError):
    """Raised when the compact C ABI returns a non-zero status."""


class _CompactBuffer(ctypes.Structure):
    _fields_ = [
        ("ptr", ctypes.POINTER(ctypes.c_ubyte)),
        ("len", ctypes.c_size_t),
        ("capacity", ctypes.c_size_t),
    ]


def _candidate_library_paths() -> list[Path]:
    here = Path(__file__).resolve()
    root = here.parents[2]
    names = ["libcompact_ffi.so", "libcompact_ffi.dylib", "compact_ffi.dll"]

    paths: list[Path] = []
    if value := os.environ.get("COMPACT_FFI_LIB"):
        paths.append(Path(value))
    for name in names:
        paths.append(root / "target" / "debug" / name)
        paths.append(root / "target" / "release" / name)
    return paths


def _load_library() -> ctypes.CDLL:
    for path in _candidate_library_paths():
        if path.exists():
            lib = ctypes.CDLL(str(path))
            _configure_library(lib)
            return lib

    raise CompactError(
        "compact FFI library not found; set COMPACT_FFI_LIB or run cargo build -p compact-ffi"
    )


def _configure_library(lib: ctypes.CDLL) -> None:
    lib.compact_version.argtypes = []
    lib.compact_version.restype = ctypes.c_char_p

    lib.compact_status_message.argtypes = [ctypes.c_int]
    lib.compact_status_message.restype = ctypes.c_char_p

    lib.compact_buffer_free.argtypes = [ctypes.POINTER(_CompactBuffer)]
    lib.compact_buffer_free.restype = None

    lib.compact_encode_bytes_rle.argtypes = [
        ctypes.POINTER(ctypes.c_ubyte),
        ctypes.c_size_t,
        ctypes.POINTER(_CompactBuffer),
    ]
    lib.compact_encode_bytes_rle.restype = ctypes.c_int

    lib.compact_decode_bytes_rle.argtypes = [
        ctypes.POINTER(ctypes.c_ubyte),
        ctypes.c_size_t,
        ctypes.POINTER(_CompactBuffer),
    ]
    lib.compact_decode_bytes_rle.restype = ctypes.c_int

    c_char_p = ctypes.c_char_p
    lib.compact_encode_file.argtypes = [c_char_p, c_char_p, c_char_p]
    lib.compact_encode_file.restype = ctypes.c_int
    lib.compact_decode_file_with_schema.argtypes = [c_char_p, c_char_p, c_char_p]
    lib.compact_decode_file_with_schema.restype = ctypes.c_int


_LIB: ctypes.CDLL | None = None


def _lib() -> ctypes.CDLL:
    global _LIB
    if _LIB is None:
        _LIB = _load_library()
    return _LIB


def _check(status: int) -> None:
    if status == 0:
        return

    message = _lib().compact_status_message(status).decode("utf-8")
    raise CompactError(message)


def _bytes_ptr(data: bytes) -> tuple[ctypes.Array[ctypes.c_ubyte] | None, object]:
    if not data:
        return None, None

    array_type = ctypes.c_ubyte * len(data)
    array = array_type.from_buffer_copy(data)
    return array, array


def version() -> str:
    return _lib().compact_version().decode("utf-8")


def encode_bytes_rle(data: bytes) -> bytes:
    return _with_bytes(data, _lib().compact_encode_bytes_rle)


def decode_bytes_rle(data: bytes) -> bytes:
    return _with_bytes(data, _lib().compact_decode_bytes_rle)


def encode_file(input_path: str | os.PathLike[str], schema_path: str | os.PathLike[str], output_path: str | os.PathLike[str]) -> None:
    status = _lib().compact_encode_file(
        os.fsencode(input_path),
        os.fsencode(schema_path),
        os.fsencode(output_path),
    )
    _check(status)


def decode_file(input_path: str | os.PathLike[str], schema_path: str | os.PathLike[str], output_path: str | os.PathLike[str]) -> None:
    status = _lib().compact_decode_file_with_schema(
        os.fsencode(input_path),
        os.fsencode(schema_path),
        os.fsencode(output_path),
    )
    _check(status)


def _with_bytes(data: bytes, function: object) -> bytes:
    ptr, keepalive = _bytes_ptr(data)
    output = _CompactBuffer()
    status = function(ptr, len(data), ctypes.byref(output))
    _check(status)

    try:
        if not output.ptr or output.len == 0:
            return b""
        return ctypes.string_at(output.ptr, output.len)
    finally:
        _lib().compact_buffer_free(ctypes.byref(output))
        _ = keepalive
