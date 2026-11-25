## Downloads

| System | File |
|---|---|
| macOS, Apple silicon | `Xilo-*-macos-arm64.dmg` |
| macOS, Intel | `Xilo-*-macos-x64.dmg` |
| Windows, x64 | `Xilo-*-windows-x64.zip` |
| Windows, Arm | `Xilo-*-windows-arm64.zip` |
| Linux, x64 | `Xilo-*-linux-x64.tar.gz` |
| Linux, Arm | `Xilo-*-linux-arm64.tar.gz` |

Each download contains the app and the `xilo` command line tool. `SHA256SUMS.txt` lists the checksums.

To use `xilo` from any folder:

- **macOS**: `sudo mkdir -p /usr/local/bin && sudo ln -sf /Applications/Xilo.app/Contents/Helpers/xilo /usr/local/bin/xilo`
- **Windows**: add the `bin` folder to your `Path`.
- **Linux**: `ln -sf "$PWD/bin/xilo" ~/.local/bin/xilo` from the unpacked folder.

The downloads are not signed by Apple or Microsoft, so the system warns you the first time you open Xilo:

- **macOS**: open Xilo once, then go to System Settings → Privacy & Security and click **Open Anyway**.
- **Windows**: click **More info**, then **Run anyway**.
- **Linux**: Xilo needs BlueZ, which most desktop systems include.

The downloads do not include the Anker images and fonts. Xilo shows its own icons instead.
