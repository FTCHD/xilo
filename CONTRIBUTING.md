# Contributing

## Build

Requirements: macOS 13 or newer, Rust (the version is pinned in `rust-toolchain.toml`), and Python 3 for the model importer only.

```sh
cargo run -p xilo                  # the app
cargo run -p xilo-cli -- scan      # the command line tool, xilo
cargo test --workspace
```

### Release downloads

```sh
cargo run -p xtask -- dist                                   # this computer
cargo run -p xtask -- dist --target x86_64-apple-darwin      # another target
```

This writes the downloads to `dist/release`: a disk image on macOS, a zip file on Windows, and a .deb, .rpm and tar.gz file on Linux. Each one contains the app and the `xilo` command line tool. On Linux, it needs `cargo install cargo-deb cargo-generate-rpm`. The package settings are in `crates/app/Cargo.toml`.

To make a release:

1. Set the version in `Cargo.toml`, for example `0.2.0`, and run `cargo check` so that `Cargo.lock` has it too. Commit and push.
2. Run the `release` workflow from the Actions tab. It builds the macOS, Windows and Linux downloads for x64 and Arm, and puts them in a draft release, `v0.2.0`, with checksums.
3. Check the draft on GitHub. Then publish it, with **Set as the latest release** on. Publishing makes the tag `v0.2.0` on the commit that was built.

If you run the workflow again for the same version, the new draft replaces the old one. The workflow stops if that version is already released.

## Development

- `XILO_SIM=1 cargo run -p xilo` runs the app against simulated devices.
- `XILO_HOME=/tmp/xilo cargo run -p xilo` keeps settings, history and logs in another folder.
- `XILO_OPEN=device:<key>:settings` opens Xilo on a page. Other values: `add`, `preferences`, and the tabs `dashboard`, `history`, `automations`, `inspector`. A fourth part acts once the device is connected: `bottom` scrolls to the end, `schedule` opens the schedule editor, and `timer` opens the AC timer, for example `device:<key>:automations:schedule`.
- `XILO_WINDOW=1200x1000` opens the window at that size, for screenshots.
- `XILO_LOG=debug` shows more in the log. Logs are in `~/Library/Application Support/dev.xilo.Xilo/logs`.
- `cargo run -p xtask -- import solix-api` regenerates `crates/models/src/families/imported` from anker-solix-api.
- `cargo run -p xtask -- check-public` fails if research files or device captures are tracked by git.
- `cargo run -p xtask -- site` writes a website page for each model, from the model definitions. Run it after you change a model. CI fails if the pages are out of date.
- The README images, `.github/showcase-wall.jpg` and `.github/automations.jpg`, are made with the screenshot scripts in `.demo/`. That folder is not in the repository.

You can add or override models without rebuilding. Put TOML files in `~/Library/Application Support/dev.xilo.Xilo/models`, using the same format as `crates/models/src/families`.

## Layout

| Folder          | Content                                                               |
| --------------- | --------------------------------------------------------------------- |
| `crates/proto`  | Frames, TLV, encryption, the ZX, Prime and legacy handshakes. No I/O. |
| `crates/models` | Model definitions (TOML), decoding, command validation, detection     |
| `crates/engine` | Device actors, simulator, USB wake, alerts                            |
| `crates/ble`    | Bluetooth transport (btleplug)                                        |
| `crates/store`  | Settings, history (SQLite), capture files                             |
| `crates/app`    | The GPUI desktop app                                                  |
| `crates/cli`    | `xilo`, the command line tool                                         |
| `xtask`         | Asset extraction, model import, bundling, checks                      |
| `website`       | The website and documentation (Fumadocs). See `website/README.md`     |
