# a9-panel

A native Windows control panel for the **ATK A9 Ultimate** wireless mouse, written in Rust with [Slint](https://slint.dev).
It is an unofficial port of the ATK Hub web app (hub.atk.pro): same pages, same look, but installable,
always available in the tray and without a browser.

- Battery percentage drawn right on the tray icon; click it to open the panel.
- Pages: key settings, macros, DPI, performance, parameters, configuration and other settings.
- English and Portuguese, following the Windows language (switchable in Other Settings).
- Every write reads the block first, saves the old bytes to `%APPDATA%\a9-panel\history.txt`
  and verifies by reading back. **Undo** restores the last change.
- No firmware updates, on purpose.

## Download

Grab `a9-panel-vX.Y.Z-windows-x64.exe` from the [Releases](../../releases) page and run it. No installer, no Rust needed.

- Close ATK Hub in the browser first: it holds the receiver.
- On the first launch the app downloads the mouse picture and the OPPO Sans font (about 10 MB) from the ATK Hub CDN
  and keeps them in `%APPDATA%\a9-panel\assets`.
- The `.exe` is not code-signed, so Windows SmartScreen may warn the first time: click **More info → Run anyway**.

## Build from source

Requirements: Windows and Rust with the MSVC toolchain.

```powershell
cargo build --release
target\release\a9-panel.exe
```

Command line options:

| option | what it does |
|--------|--------------|
| `--tray` | start hidden in the tray |
| `--dump` | read everything from the mouse into `%APPDATA%\a9-panel\dump.txt` and exit (read-only) |
| `--page=N --shot=file.bmp` | open off-screen on page N, save a screenshot and exit |

Releases are built by GitHub Actions when a `v*` tag is pushed (`.github/workflows/release.yml`).

## How it works

ATK Hub talks to the mouse through WebHID from plain JavaScript. The packet format, EEPROM map, DPI
encoding, button and macro formats were read from that code and checked against a real mouse.
Everything is documented in [docs/protocol.md](docs/protocol.md).

Tested with the 8K receiver `373B:11D9` and an A9 Ultimate (CID 1, MID 3). Reads are verified on hardware;
some writes (buttons, macros, profiles, factory reset) still need more real-world testing, so try one change at a time.

## Credits and trademarks

Independent project, not affiliated with or endorsed by ATK. "ATK" and "ATK Hub" are trademarks of their owners.
The mouse picture and the OPPO Sans font belong to their owners and are **not** included in this repository
or in the `.exe`; the app downloads them on your machine.

## License

[MIT](LICENSE)
