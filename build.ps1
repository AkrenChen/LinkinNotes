$ErrorActionPreference = 'Stop'
Push-Location $PSScriptRoot
try {
    python scripts/fetch_native_dependencies.py
    if ($LASTEXITCODE) { throw 'Native dependency setup failed' }
    python -m unittest discover -s backend -v
    if ($LASTEXITCODE) { throw 'Backend tests failed' }
    python -m PyInstaller --noconfirm --onefile --name linkin-backend --distpath src-tauri/sidecar --workpath build/pyinstaller --add-binary backend/resources/sqlite3mc_x64.dll:resources --add-binary backend/resources/simple.dll:resources backend/service.py
    if ($LASTEXITCODE) { throw 'Backend build failed' }
    npm.cmd run build
    if ($LASTEXITCODE) { throw 'Frontend build failed' }
    cargo build --release --manifest-path src-tauri/Cargo.toml
    if ($LASTEXITCODE) { throw 'Native build failed' }
    New-Item -ItemType Directory -Force release/LinkinNotes/sidecar | Out-Null
    Copy-Item src-tauri/target/release/linkin-notes.exe release/LinkinNotes/LinkinNotes.exe -Force
    Copy-Item src-tauri/sidecar/linkin-backend.exe release/LinkinNotes/sidecar/ -Force
} finally { Pop-Location }
