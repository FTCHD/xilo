# Xilo

![Xilo with simulated devices: the C300X dashboard in front of a wall of other screens](.demo/showcase-wall.jpg)

Xilo is a desktop app for Anker SOLIX devices. It talks to them over Bluetooth only. You don't need an account, a cloud connection or the internet.

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

| Kind            | Models                                                                                                                                                                                                                      |
| --------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Power stations  | C300, C300X, C300 DC, C300X DC, C200(X), C200 DC, C200X DC, C800, C800 Plus, C800X, C1000(X), C1000 Gen 2, C1000X Gen 2, C2000 Gen 2, C2000X Gen 2, S2000, F1200, F1500, F2000, F2000 Plus, F2600, F3000, F3800, F3800 Plus |
| Solarbanks      | E1600, 2 Pro, 2 AC, 2 Plus, 3 Pro, 4 Pro                                                                                                                                                                                    |
| Home            | Home Power Panel                                                                                                                                                                                                            |
| Prime and other | Prime Charger 160W and 250W, Prime Power Bank 20K, MagGo 3-in-1, Prime Charging Station 240W, EverFrost 2 40L and 58L                                                                                                       |

Most of these maps come from [anker-solix-api](https://github.com/thomluther/anker-solix-api) and [SolixBLE](https://github.com/flip-dots/SolixBLE). Their published test payloads decode to the same values in Xilo (`crates/models/tests/solixble_values.rs`).

Protocols: ZX with ECDH and AES-CBC, ZX Gen 2, Prime with AES-GCM, the older AES key exchange, and the 2540 GATT service.

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

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for building, development and the layout of the code.
