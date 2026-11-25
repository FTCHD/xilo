# Xilo

Xilo is a desktop app for Anker SOLIX devices. It talks to them over Bluetooth only. You don't need an account, a cloud connection or the internet.

It runs on macOS. The code is written to be cross-platform, but Windows and Linux are not built or tested yet.

Xilo is not made by or affiliated with Anker.

## Features

- **Dashboards** for the C300X and C1000 that follow the Anker app: battery, temperature, remaining time, port status, AC and car outputs with live graphs, light and screen controls, and error messages.
- **Every setting** from the Anker app, with option lists and limits checked before anything is sent. Risky changes (AC frequency, factory reset) ask for confirmation, and each one is written to an audit log.
- **38 models** in total. Models without their own dashboard get a generic one with all decoded values.
- **Schedules**: turn outputs, the light or the screen on or off at set times and on chosen days.
- **Battery reserve**: when the battery drops to a set level, Xilo turns off the outputs you choose, so some charge stays for later.
- **USB wake**: the C300X turns its USB ports off after about two hours of low load. Xilo keeps them on by switching the car output on briefly before that happens.
- **Alerts**: errors, low battery, a chosen charge level (80%, 90% or 100%), AC power lost and back, and long disconnects. They show in the app and as system notifications.
- Automations and alerts run while Xilo is open or in the menu bar, and the device is in Bluetooth range.
- **History**: battery and power every 10 seconds, stored on this Mac. It includes charts, energy per day and CSV export.
- **Menu bar**: battery level in the menu bar, quick AC and car output toggles, and "release to phone" so the Anker app can connect.
- **Inspector** (developer mode): raw and decrypted frames, a per-message diff, redacted capture files and a guarded raw command console.
- **`xilo`**: the same engine on the command line.

## Supported devices

| Kind | Models |
|---|---|
| Power stations | C300, C300X, C300 DC, C300X DC, C200(X), C200 DC, C200X DC, C800, C800 Plus, C800X, C1000(X), C1000 Gen 2, C1000X Gen 2, C2000 Gen 2, C2000X Gen 2, S2000, F1200, F1500, F2000, F2000 Plus, F2600, F3000, F3800, F3800 Plus |
| Solarbanks | E1600, 2 Pro, 2 AC, 2 Plus, 3 Pro, 4 Pro |
| Home | Home Power Panel |
| Prime and other | Prime Charger 160W and 250W, Prime Power Bank 20K, MagGo 3-in-1, Prime Charging Station 240W, EverFrost 2 40L and 58L |

Most of these maps come from [anker-solix-api](https://github.com/thomluther/anker-solix-api) and [SolixBLE](https://github.com/flip-dots/SolixBLE). Their published test payloads decode to the same values in Xilo (`crates/models/tests/solixble_values.rs`).

Protocols: ZX with ECDH and AES-CBC, ZX Gen 2, Prime with AES-GCM, the older AES key exchange, and the 2540 GATT service. See [docs/variants.md](docs/variants.md).

## Build

Requirements: macOS 13 or newer, Rust (the version is pinned in `rust-toolchain.toml`), and Python 3 for the model importer only.

```sh
cargo run -p xilo                  # the app
cargo run -p xilo-cli -- scan      # the command line tool, xilo
cargo test --workspace
```

### Anker images and fonts

The dashboards use images, fonts and animations from the Anker Android app. They are not in this repository. To add them, put the unpacked `.xapk` in `.research/anker-apk/xapk/` and run:

```sh
cargo run -p xtask -- assets extract
```

Without them, Xilo shows its own icons and the system font.

### App bundle

```sh
cargo run -p xtask -- bundle --release   # creates dist/Xilo.app
```

The bundle is signed ad hoc. macOS then asks for Bluetooth permission again after every rebuild. To avoid that, create a code signing certificate in Keychain Access and set `XILO_SIGN_IDENTITY` to its name before you bundle.

### Release downloads

```sh
cargo run -p xtask -- dist                                   # this computer
cargo run -p xtask -- dist --target x86_64-apple-darwin      # another target
```

This writes the downloads to `dist/release`: a disk image on macOS, a zip file on Windows, and a .deb, .rpm and tar.gz file on Linux. Each one contains the app and the `xilo` command line tool. On Linux, it needs `cargo install cargo-deb cargo-generate-rpm`. The package settings are in `crates/app/Cargo.toml`. The downloads never include the Anker images and fonts, even when `assets/vendor` exists.

To publish a release, set the version in `Cargo.toml` and push a tag with the same version, for example `v0.2.0`. The `release` workflow then builds macOS, Windows and Linux downloads for x64 and Arm, and creates a GitHub release with checksums. To build the downloads without a release, run the workflow by hand from the Actions tab.

## Command line

In the downloads, `xilo` is in `Xilo.app/Contents/Helpers/` on macOS and in `bin/` on Windows and Linux.

```sh
xilo scan
xilo --name C300X status
xilo --name C300X watch --secs 600
xilo --name C300X set light_mode 1
xilo --name C300X set ac_frequency 60 --confirm
xilo --name C300X capture --secs 30 -o c300x.xcap
xilo --sim A1723 status            # simulated device, no Bluetooth
xilo models
```

## Development

- `XILO_SIM=1 cargo run -p xilo` runs the app against simulated devices.
- `XILO_HOME=/tmp/xilo cargo run -p xilo` keeps settings, history and logs in another folder.
- `XILO_OPEN=device:<key>:settings` opens Xilo on a page. Other values: `add`, `preferences`, `about`, and the tabs `dashboard`, `history`, `automations`, `inspector`.
- `XILO_LOG=debug` shows more in the log. Logs are in `~/Library/Application Support/dev.xilo.Xilo/logs`.
- `cargo run -p xtask -- import solix-api` regenerates `crates/models/src/families/imported` from anker-solix-api.
- `cargo run -p xtask -- check-public` fails if Anker files or captures are tracked by git.

You can add or override models without rebuilding. Put TOML files in `~/Library/Application Support/dev.xilo.Xilo/models`, using the same format as `crates/models/src/families`.

## Layout

| Folder | Content |
|---|---|
| `crates/proto` | Frames, TLV, encryption, the ZX, Prime and legacy handshakes. No I/O. |
| `crates/models` | Model definitions (TOML), decoding, command validation, detection |
| `crates/engine` | Device actors, simulator, USB wake, alerts |
| `crates/ble` | Bluetooth transport (btleplug) |
| `crates/store` | Settings, history (SQLite), capture files |
| `crates/app` | The GPUI desktop app |
| `crates/cli` | `xilo`, the command line tool |
| `xtask` | Asset extraction, model import, bundling, checks |
| `docs` | Protocol and device notes |

## Licenses

Xilo ports protocol logic and field maps from SolixBLE (MIT, © 2026 Harvey Lelliott) and anker-solix-api (MIT, © 2024 thomluther). See [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
