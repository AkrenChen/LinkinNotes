import ctypes as C
import pathlib
ROOT = pathlib.Path(__file__).parent

class NativeDatabase:
    def __init__(self, source, key=None):
        self.library = C.CDLL(str(ROOT / "resources" / "sqlite3mc_x64.dll"))
        declarations = {
            "sqlite3_open_v2": ([C.c_char_p, C.c_void_p, C.c_int, C.c_char_p], C.c_int),
            "sqlite3_close": ([C.c_void_p], C.c_int),
            "sqlite3_errmsg": ([C.c_void_p], C.c_char_p),
            "sqlite3mc_cipher_index": ([C.c_char_p], C.c_int),
            "sqlite3mc_config": ([C.c_void_p, C.c_char_p, C.c_int], C.c_int),
            "sqlite3mc_config_cipher": ([C.c_void_p, C.c_char_p, C.c_char_p, C.c_int], C.c_int),
            "sqlite3_key": ([C.c_void_p, C.c_void_p, C.c_int], C.c_int),
            "sqlite3_busy_timeout": ([C.c_void_p, C.c_int], C.c_int),
            "sqlite3_prepare_v2": ([C.c_void_p, C.c_char_p, C.c_int, C.c_void_p, C.c_void_p], C.c_int),
            "sqlite3_bind_text": ([C.c_void_p, C.c_int, C.c_char_p, C.c_int, C.c_void_p], C.c_int),
            "sqlite3_bind_int64": ([C.c_void_p, C.c_int, C.c_int64], C.c_int),
            "sqlite3_bind_pointer": ([C.c_void_p, C.c_int, C.c_void_p, C.c_char_p, C.c_void_p], C.c_int),
            "sqlite3_step": ([C.c_void_p], C.c_int),
            "sqlite3_column_count": ([C.c_void_p], C.c_int),
            "sqlite3_column_name": ([C.c_void_p, C.c_int], C.c_char_p),
            "sqlite3_column_type": ([C.c_void_p, C.c_int], C.c_int),
            "sqlite3_column_text": ([C.c_void_p, C.c_int], C.c_char_p),
            "sqlite3_column_int64": ([C.c_void_p, C.c_int], C.c_int64),
            "sqlite3_finalize": ([C.c_void_p], C.c_int),
        }
        for name, (arguments, result) in declarations.items():
            function = getattr(self.library, name)
            function.argtypes = arguments
            function.restype = result
        self.handle = C.c_void_p()
        self.check(self.library.sqlite3_open_v2(str(source).encode(), C.byref(self.handle), 2 | 0x10000, None))
        if key is not None:
            cipher = self.library.sqlite3mc_cipher_index(b"chacha20")
            self.library.sqlite3mc_config(self.handle, b"cipher", cipher)
            self.library.sqlite3mc_config_cipher(self.handle, b"chacha20", b"legacy", 0)
            raw = b"raw:" + key
            self.check(self.library.sqlite3_key(self.handle, raw, len(raw)))
        self.check(self.library.sqlite3_busy_timeout(self.handle, 5000))

        self.library.sqlite3_enable_load_extension.argtypes = [C.c_void_p, C.c_int]
        self.library.sqlite3_load_extension.argtypes = [C.c_void_p, C.c_char_p, C.c_char_p, C.c_void_p]
        self.check(self.library.sqlite3_enable_load_extension(self.handle, 1))
        self.check(self.library.sqlite3_load_extension(self.handle, str(ROOT / "resources" / "simple.dll").encode(), None, None))
        self.check(self.library.sqlite3_enable_load_extension(self.handle, 0))

    def check(self, result):
        if result:
            raise RuntimeError(f"SQLite {result}: " + self.library.sqlite3_errmsg(self.handle).decode(errors="replace"))

    def query(self, sql, parameters=()):
        statement = C.c_void_p()
        self.check(self.library.sqlite3_prepare_v2(self.handle, sql.encode(), -1, C.byref(statement), None))
        rows = []
        try:
            for index, value in enumerate(parameters, 1):
                if isinstance(value, int):
                    self.check(self.library.sqlite3_bind_int64(statement, index, value))
                else:
                    data = str(value).encode()
                    self.check(self.library.sqlite3_bind_text(statement, index, data, len(data), C.c_void_p(-1)))
            while True:
                result = self.library.sqlite3_step(statement)
                if result == 101:
                    break
                if result != 100:
                    self.check(result)
                row = {}
                for column in range(self.library.sqlite3_column_count(statement)):
                    name = self.library.sqlite3_column_name(statement, column).decode()
                    kind = self.library.sqlite3_column_type(statement, column)
                    value = self.library.sqlite3_column_int64(statement, column) if kind == 1 else None if kind == 5 else (self.library.sqlite3_column_text(statement, column) or b"").decode()
                    row[name] = value
                rows.append(row)
        finally:
            self.library.sqlite3_finalize(statement)
        return rows

    def close(self):
        self.check(self.library.sqlite3_close(self.handle))

