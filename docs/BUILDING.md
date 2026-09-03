# Building Clipbox

These notes cover local development. The project is still small, so there is no separate production deployment process yet.

## Requirements

Install the following before starting:

- Rust and Cargo
- Node.js and npm
- The platform dependencies required by [Tauri 2](https://v2.tauri.app/start/prerequisites/)

On Windows, this also means WebView2 and the MSVC C++ build tools. On Linux, install the GTK/WebKit development packages listed in the Tauri guide.

## Run the app

From the repository root:

```sh
npm install
npm run tauri dev
```

Clipbox will open a desktop window. Copy a new piece of text from another application while it is running, then wait briefly for the clipboard monitor to see it.

The clipboard content already present when Clipbox starts is used as the baseline and is not imported automatically.

## Check the code

```sh
cargo fmt --all -- --check
cargo check --workspace
cargo test --workspace
```

To build the desktop executable:

```sh
npm run tauri build
```

On Windows this also produces an NSIS installer (`Clipbox_*_x64-setup.exe` under `target/release/bundle/nsis/`). Building the installer requires [NSIS 3](https://nsis.sourceforge.io/) on top of the regular prerequisites.

## Find the database

Clipbox creates `clipbox.sqlite3` in Tauri's application-data directory:

- Windows: `%APPDATA%\com.palethea.clipbox\clipbox.sqlite3`
- macOS: `~/Library/Application Support/com.palethea.clipbox/clipbox.sqlite3`
- Linux: `~/.local/share/com.palethea.clipbox/clipbox.sqlite3`

On Windows, if Python is installed, print the stored entries with:

```powershell
$db = Join-Path $env:APPDATA 'com.palethea.clipbox\clipbox.sqlite3'
python -c 'import sqlite3,sys; print(*sqlite3.connect(sys.argv[1]).execute("SELECT id, datetime(copied_at, ''unixepoch'', ''localtime''), source_app, source_process, window_title, content FROM clipboard_entries ORDER BY id DESC").fetchall(), sep="\n")' $db
```
