# QCY Earbuds for Linux and Windows

Control QCY wireless earbuds from your computer: noise cancelling, transparency, EQ,
touch controls, dual-device connection, LDAC, wear detection and more, without
the phone app.

Written in Rust. On Linux it talks to BlueZ over D-Bus (pure Rust: no
`libdbus`, no root); on Windows it uses the built-in Bluetooth LE API. No QCY
account is needed on either. The protocol was reverse-engineered from
the official Android app and verified on real hardware
([docs/PROTOCOL.md](docs/PROTOCOL.md)).

| Noise control | Sound | Touch controls | Settings |
|---|---|---|---|
| ![](docs/screenshots/noise.png) | ![](docs/screenshots/sound.png) | ![](docs/screenshots/controls.png) | ![](docs/screenshots/settings.png) |
| ![](docs/screenshots/light-noise.png) | ![](docs/screenshots/custom-eq.png) | ![](docs/screenshots/light-dialog.png) | ![](docs/screenshots/light-settings.png) |

## What you get

| Program    | What it is                                                                 |
|------------|----------------------------------------------------------------------------|
| `qcy-gui`  | The control panel window, in Material 3 style with light and dark themes (follows the system, or pick one under Settings → Appearance). |
| `qcy-tray` | A tray icon (KDE Plasma or any StatusNotifier host on Linux; the notification area on Windows). The tooltip shows battery levels; the menu offers noise control, gaming mode and EQ presets; clicking the icon opens the control panel. On Linux, scrolling over it cycles noise control. |
| `qcyctl`   | Command line tool for scripts and keyboard shortcuts.                      |

### Supported features (QCY MeloBuds Pro)

Tested on real earbuds (firmware 2.0.6):

- Reading every setting, battery of both buds and the case, firmware versions
- Noise control: off, noise cancelling (adaptive, indoor, commuting, noisy,
  anti-wind, with strength levels), transparency levels 1–6, voice focus
- Equalizer: factory presets and a 10-band custom EQ with a live response
  curve (automatic anti-clipping gain, same as the official app)
- Touch controls: single, double and triple tap on each bud
- Gaming (low latency) mode

Implemented byte-for-byte from the official app, but not yet exercised on
hardware:

- Left/right balance, voice prompt language and volume
- Sleep mode, LDAC, dual device connection (the last two reboot the earbuds)
- Wear detection (pause/resume behaviour, ANC follows wear)
- Auto power-off after disconnect, sleep timer
- Rename, restore defaults, factory reset

Other QCY models that use the same protocol (WuQi, Realtek and most other
chips) work with a generic profile: battery, touch controls, plus any
setting they report. Each extra model needs a profile to unlock its full
feature set; see [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md#support-another-qcy-model).
Models with JieLi or ZR chips use different protocols and are not supported
yet.

**Firmware updates are deliberately not supported.**

## Requirements

**Linux**

- BlueZ 5 (`bluetoothd` running, adapter powered; tested with 5.87)
- For the GUI: a Wayland or X11 session with OpenGL. The libraries ship with
  any desktop (`libxkbcommon`, `libwayland-client`, `libGL`/`libEGL`).
- To build: Rust ≥ 1.85. No `-dev` packages are needed.

**Windows**

- Windows 10 version 2004 or newer, or Windows 11
- A Bluetooth adapter with Bluetooth Low Energy, using the standard Microsoft
  Bluetooth driver (nearly every laptop)
- Nothing to install: the programs use only DLLs that ship with Windows.

## Build and install on Linux

```bash
packaging/install.sh
```

This builds in release mode and installs to `~/.local/bin`, plus menu entries.
To also start the tray icon on login:

```bash
packaging/install.sh --autostart
```

Remove everything with `packaging/uninstall.sh`. Or just build and run in place:

```bash
cargo build --release
./target/release/qcy-gui
```

To try the interface without earbuds, run `qcy-gui --demo`. It simulates a
MeloBuds Pro using data captured from real hardware.

## Build for Windows (from Linux)

The Windows version is cross-compiled; no Windows machine is needed to build.

```bash
rustup target add x86_64-pc-windows-gnu   # once
sudo apt install mingw-w64                # once, if x86_64-w64-mingw32-gcc is missing
packaging/build-windows.sh
```

This produces `dist/qcy-earbuds-<version>-windows-x64.zip` with `qcy-gui.exe`,
`qcy-tray.exe`, `qcyctl.exe` and an installer script. On the Windows PC, unzip
it and run:

```powershell
powershell -ExecutionPolicy Bypass -File install.ps1 -Autostart
```

`install.ps1` copies the programs to `%LOCALAPPDATA%\Programs\QCY Earbuds`,
adds Start menu shortcuts and, with `-Autostart`, starts the tray at sign-in
(`-AddToPath` makes `qcyctl` available in terminals). No administrator rights
are needed. `uninstall.ps1` removes it all again. The `.exe` files also run
directly from the unzipped folder.

Building on Windows itself works too: install Rust (rustup) and the Visual
Studio Build Tools ("Desktop development with C++"), then `cargo build
--release`.

## Usage

Pair the earbuds with your computer as usual (for audio). Then take them out
of the case and start `qcy-gui` or `qcy-tray`. The earbuds are found
automatically.

The earbuds accept only **one control connection**. If the QCY app on your
phone is connected, close it first. The programs on your computer share one
connection with each other, so the tray and the window can run together.

### Command line

```bash
qcyctl                                  # status: battery, firmware, every setting
qcyctl scan                             # list QCY earbuds nearby
qcyctl anc on --scene adaptive          # noise cancelling
qcyctl anc on --scene commuting --level 2
qcyctl anc transparency --level 4
qcyctl anc voice                        # transparency with voice focus
qcyctl anc off
qcyctl eq list                          # presets of this model
qcyctl eq preset "Heavy Bass"
qcyctl eq custom 3 2 1 0 0 0 0 1 2 3    # 10 bands, 31 Hz … 16 kHz, −8…+8 dB
qcyctl toggle dual-device on            # also: game-mode, sleep-mode, ldac
qcyctl keys                             # show touch controls
qcyctl keys set right-double next
qcyctl wear-detection on --music pause-resume --anc on
qcyctl balance 50
qcyctl auto-off 30                      # minutes, or "never"
qcyctl sleep-timer 60                   # 0 cancels
qcyctl monitor                          # live view until Ctrl+C
```

Global options: `--device <MAC>` picks specific earbuds (audio or BLE
address), `--keep-connected` keeps the control link open for faster repeated
calls, and `-v`/`-vv` print debug logs or a full protocol trace.

Example KDE shortcut (System Settings → Shortcuts → Add command):
`qcyctl anc on --scene adaptive`.

## Troubleshooting

| Symptom | Fix |
|---|---|
| "No QCY earbuds found" | Take the buds out of the case; close the QCY phone app; make sure Bluetooth is on (Linux: `bluetoothctl show` reports `Powered: yes`). |
| Connecting takes long | BLE connects can take 5–40 s while audio is streaming; the programs retry on their own. |
| Buds disconnect after toggling LDAC / dual device | Expected: the firmware reboots. The GUI and tray reconnect on their own. |
| Rename has no visible effect | Reconnect audio; phones cache names. Keep both buds out of the case while renaming. |
| "Bluetooth error" on Windows | Check Bluetooth is on in Settings and the adapter supports Bluetooth LE. Older adapters with vendor Bluetooth software (e.g. WIDCOMM) are not supported by Windows' BLE API. |
| Need details | Run with `RUST_LOG=debug` (GUI/tray) or `qcyctl -vv`. |

## Project layout

```
crates/
  qcy-protocol/   pure protocol: framing, commands, decoding, EQ math, model profiles
  qcy-client/     stateful client, connection manager and the Bluetooth backends
                  (BlueZ on Linux, WinRT on Windows)
  qcyctl/         CLI
  qcy-gui/        egui control panel
  qcy-tray/       tray applet
docs/
  PROTOCOL.md     full protocol reference
  ARCHITECTURE.md design and how to extend it
assets/           application icon
packaging/        Linux install scripts, Windows build script and installer
research/         decompiled app, captured data and logs (see research/README.md)
```

## Disclaimer

Unofficial project, not affiliated with QCY. Protocol knowledge comes from
interoperability analysis of the publicly distributed Android app. Use at your
own risk.
