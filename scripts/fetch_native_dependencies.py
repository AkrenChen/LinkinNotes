import hashlib
import io
import pathlib
import urllib.request
import zipfile


DEPENDENCIES = [
    (
        'https://github.com/utelle/SQLite3MultipleCiphers/releases/download/v2.6.1/sqlite3mc-2.6.1-sqlite-3.54.0-win64.zip',
        'dll/sqlite3mc_x64.dll',
        '683927db7e16cca301c5f2a236830de28c26b47f01d948f6abf2f0d60776ce4d',
    ),
    (
        'https://github.com/wangfenjin/simple/releases/download/v0.7.1/libsimple-windows-x64.zip',
        'libsimple-windows-x64/simple.dll',
        '27c700ca34cd5935ff934459f1f9c107cdefef7e54250de5a1c6646e05f21a4f',
    ),
]


def main():
    destination = pathlib.Path(__file__).resolve().parents[1] / 'backend' / 'resources'
    destination.mkdir(parents=True, exist_ok=True)
    for url, member, expected in DEPENDENCIES:
        target = destination / pathlib.PurePosixPath(member).name
        if target.exists() and hashlib.sha256(target.read_bytes()).hexdigest() == expected:
            print(f'{target.name}: verified')
            continue
        request = urllib.request.Request(url, headers={'User-Agent': 'LinkinNotes-build'})
        with urllib.request.urlopen(request, timeout=60) as response:
            archive = response.read()
        with zipfile.ZipFile(io.BytesIO(archive)) as bundle:
            data = bundle.read(member)
        if hashlib.sha256(data).hexdigest() != expected:
            raise RuntimeError(f'{target.name}: SHA-256 mismatch')
        target.write_bytes(data)
        print(f'{target.name}: downloaded and verified')


if __name__ == '__main__':
    main()
