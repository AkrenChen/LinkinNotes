import argparse
import ctypes
import hashlib
import json
import pathlib
import re
import sqlite3
import struct
import time

from Crypto.Cipher import ChaCha20


class MemoryRegion(ctypes.Structure):
    _fields_ = [
        ("BaseAddress", ctypes.c_void_p),
        ("AllocationBase", ctypes.c_void_p),
        ("AllocationProtect", ctypes.c_ulong),
        ("PartitionId", ctypes.c_ushort),
        ("RegionSize", ctypes.c_size_t),
        ("State", ctypes.c_ulong),
        ("Protect", ctypes.c_ulong),
        ("Type", ctypes.c_ulong),
    ]


def stream(key, nonce, counter, data):
    cipher = ChaCha20.new(key=key, nonce=nonce[:12])
    cipher.seek(counter * 64)
    return cipher.decrypt(data)


def poly1305(message, key):
    r = int.from_bytes(key[:16], "little") & 0x0FFFFFFC0FFFFFFC0FFFFFFC0FFFFFFF
    s = int.from_bytes(key[16:32], "little")
    accumulator = 0
    for offset in range(0, len(message), 16):
        block = message[offset:offset + 16]
        accumulator = ((accumulator + int.from_bytes(block + b"\x01", "little")) * r) % (2**130 - 5)
    return ((accumulator + s) % 2**128).to_bytes(16, "little")


def decrypt_page(page, number, key):
    end = len(page) - 32
    nonce = page[end:end + 16]
    counter = struct.unpack_from("<I", nonce, 12)[0] ^ number
    one_time = stream(key, nonce, counter, bytes(64))
    if poly1305(page[:end + 16], one_time[:32]) != page[end + 16:]:
        raise ValueError(f"Authentication failed on page {number}")
    offset = 24 if number == 1 else 0
    plain = bytearray(page)
    plain[offset:end] = stream(one_time[32:], nonce, (counter + 1) & 0xFFFFFFFF, page[offset:end])
    plain[end:] = bytes(32)
    if number == 1:
        plain[:16] = b"SQLite format 3\x00"
    return plain


def find_keys(pid, databases):
    kernel = ctypes.WinDLL("kernel32", use_last_error=True)
    kernel.OpenProcess.restype = ctypes.c_void_p
    kernel.OpenProcess.argtypes = [ctypes.c_ulong, ctypes.c_int, ctypes.c_ulong]
    kernel.ReadProcessMemory.argtypes = [ctypes.c_void_p, ctypes.c_void_p, ctypes.c_void_p, ctypes.c_size_t, ctypes.c_void_p]
    kernel.VirtualQueryEx.argtypes = [ctypes.c_void_p, ctypes.c_void_p, ctypes.c_void_p, ctypes.c_size_t]
    kernel.VirtualQueryEx.restype = ctypes.c_size_t
    kernel.CloseHandle.argtypes = [ctypes.c_void_p]
    handle = kernel.OpenProcess(0x410, False, pid)
    if not handle:
        raise ctypes.WinError(ctypes.get_last_error())
    salts = {data[:16]: name for name, data in databases.items()}
    pattern = re.compile(b"|".join(re.escape(salt) for salt in salts))
    found = {}
    scanned = 0
    hits = 0
    address = 0
    deadline = time.monotonic() + 45
    try:
        while address < 0x100000000 and time.monotonic() < deadline:
            region = MemoryRegion()
            if not kernel.VirtualQueryEx(handle, address, ctypes.byref(region), ctypes.sizeof(region)):
                break
            start = region.BaseAddress or address
            address = start + region.RegionSize
            if region.State != 0x1000 or region.Type != 0x20000 or region.Protect & 0x100 or region.Protect & 0xFF not in (4, 8, 0x40, 0x80):
                continue
            for offset in range(0, region.RegionSize, 1024 * 1024):
                size = min(1024 * 1024 + 128, region.RegionSize - offset)
                buffer = ctypes.create_string_buffer(size)
                read = ctypes.c_size_t()
                if not kernel.ReadProcessMemory(handle, start + offset, buffer, size, ctypes.byref(read)):
                    continue
                data = buffer.raw[:read.value]
                scanned += len(data)
                for match in pattern.finditer(data):
                    position = match.start()
                    hits += 1
                    if position < 36 or struct.unpack_from("<I", data, position - 36)[0] != 32:
                        continue
                    name = salts[match.group()]
                    key = data[position - 32:position]
                    page_size = int.from_bytes(databases[name][16:18], "big")
                    try:
                        decrypt_page(databases[name][:page_size], 1, key)
                    except ValueError:
                        continue
                    found[name] = key
                if len(found) == len(databases):
                    break
            if len(found) == len(databases):
                break
    finally:
        kernel.CloseHandle(handle)
    return found

