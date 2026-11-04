# Native dependencies

The source repository does not redistribute native binaries. `scripts/fetch_native_dependencies.py` downloads pinned upstream Windows x64 artifacts and verifies the SHA-256 of each extracted DLL before use.

| Component | Version | Upstream | License |
| --- | --- | --- | --- |
| SQLite3 Multiple Ciphers | 2.6.1 / SQLite 3.54.0 | [release](https://github.com/utelle/SQLite3MultipleCiphers/releases/tag/v2.6.1) | [upstream MIT license and component notices](https://github.com/utelle/SQLite3MultipleCiphers/blob/main/LICENSE) |
| simple SQLite tokenizer | 0.7.1 | [release](https://github.com/wangfenjin/simple/releases/tag/v0.7.1) | [upstream license](https://github.com/wangfenjin/simple/blob/master/LICENSE) |

The tokenizer is required by the existing database's FTS5 triggers when tasks are created or edited, even though this application has no search feature. No external dictionary is loaded by this application.

Frontend and Rust dependency versions are recorded in `package-lock.json` and `src-tauri/Cargo.lock`. Python dependencies are pinned in `backend/requirements.txt`. Refer to the respective upstream packages for their license terms. When distributing a packaged application, include the required notices for bundled third-party components.
