# Architecture

```
┌──────────────┐  ┌───────────────────┐  ┌──────────────┐
│   qcy-gui    │  │     qcy-tray      │  │    qcyctl    │   front-ends
│ egui window  │  │ KDE SNI / Windows │  │ command line │
└──────┬───────┘  └─────────┬─────────┘  └──────┬───────┘
       │ Manager            │ Manager           │ Earbuds (direct)
┌──────▼────────────────────▼───────────────────▼─────────┐
│ qcy-client                                               │   client
│  client    ─ Earbuds: live DeviceState + commands        │
│  manager   ─ background (re)connect, snapshots           │
│  transport ─ Backend / Link traits, Endpoint, Selector   │
│  backend/  ─ bluez (Linux)  │  winrt (Windows)            │
└──────┬───────────────────────────────┬──────────────────┘
       │ bytes                          │
┌──────▼────────────────────────────┐   │
│ qcy-protocol (pure, no I/O)       │   │                     protocol
│  frame · ids · types · eq ·       │   │
│  command · update · state ·       │   │
│  advert · profile · samples       │   │
└───────────────────────────────────┘   │
        Linux: D-Bus (zbus) → BlueZ ─┐  │  Windows: Windows.Devices.Bluetooth
                                     ▼  ▼
                         BLE ── earbuds, service 0xA001
```

## Crates

| Crate          | Responsibility |
|----------------|----------------|
| `qcy-protocol` | Turns typed [`Command`]s into bytes and bytes into typed [`Update`]s. Holds model profiles and captured sample data. No Bluetooth code at all. |
| `qcy-client`   | The stateful `Earbuds` client and the long-running `Manager`, written against two traits ([`Backend`, `Link`]). Implements them for BlueZ (`backend/bluez`, Linux) and WinRT (`backend/winrt`, Windows); `Platform` is the one for the build target. |
| `qcyctl`       | One-shot CLI on top of `Earbuds`. Good for scripts and keyboard shortcuts. |
| `qcy-gui`      | egui control panel on top of `Manager`. Its `material/` module is a small, app-independent Material 3 design system (colour roles, type scale, Material Symbols icons, cards, switches, chips, segmented buttons, navigation rail, snackbars, dialogs); `views/` are the screens and `widgets.rs` the app-specific drawings (battery rings, EQ curve). |
| `qcy-tray`     | Tray applet on top of `Manager`. `menu.rs` decides what is shown and done; `sni.rs` draws it as a KDE StatusNotifierItem (Linux), `win.rs` as a notification-area icon (Windows). |

## Data flow

1. **Discovery** (`Backend::find`): collects QCY advertisements (company id
   `0x521C`), parses them into `Advertisement`s and matches their embedded
   classic address to paired audio devices. BlueZ keeps a system-wide device
   cache; on Windows an advertisement watcher feeds the backend's own cache,
   persisted in `%LOCALAPPDATA%\qcy-earbuds\endpoints.txt` so a second
   program can find already-connected earbuds (which stop advertising).
   `Endpoint::pick` ranks candidates: already-connected link, then the headset
   connected for audio, then paired, then any.
2. **Link** (`Backend::open`): connects without cancelling slow attempts
   (BlueZ: `Connect` + `Connected` watch; Windows: uncached service discovery
   with retries) and maps the characteristics of service `0xA001` only.
3. **Client** (`client.rs`): `Earbuds::connect` subscribes to notifications,
   decodes every value with `qcy_protocol::update::decode`, and folds the
   results into a `DeviceState` published through a `tokio::sync::watch`.
   `send(Command)` serialises writes, applies an optimistic update, and asks
   the earbuds to confirm one second later.
4. **Manager** (`manager.rs`): owns the client in a background task, retries
   with backoff while the user wants to be connected (earbuds reboot after
   LDAC / dual-device changes and vanish into the case), and exposes
   `Snapshot`s plus `Notice`s for snackbars.
5. **Front-ends** render snapshots and push commands. The GUI's views are
   pure: they return `Action`s that `App` executes after the frame.

## Design rules

* **Protocol knowledge lives only in `qcy-protocol`.** Front-ends never build
  bytes; they use `Command`, `DeviceState` and `DeviceProfile`.
* **Platform code lives only in `qcy-client/src/backend/`.** Everything above
  the `Backend` / `Link` traits is shared by all operating systems.
* **Unknown data is preserved.** Unknown setting ids, key actions, ANC
  triples and wear behaviours round-trip as `Other(..)` / `Unknown { .. }`
  instead of being dropped.
* **Writes are confirmed, not trusted.** The device is the source of truth;
  optimistic updates are corrected by its reply.
* **Never touch firmware.** Backends only resolve characteristics of service
  `0xA001`; the OTA service is unreachable by construction.

## Extending

### Support another QCY model

1. Find its model id: `qcyctl scan` prints `model=0x…`.
2. Fetch its control-panel JSON (see `docs/PROTOCOL.md` §10) and save it
   under `research/panel_json/`.
3. Add a `DeviceProfile` constant in `crates/qcy-protocol/src/profile.rs`
   (copy `MELOBUDS_PRO`) and list it in `PROFILES`.
4. If the model is JieLi- or ZR-based (`ChipFamily`), it speaks a different
   protocol and needs its own encoder/decoder first.

Until a profile exists the model falls back to `GENERIC`, and the GUI still
shows any toggle the earbuds report on their own.

### Add a setting

1. Add its id to `ids.rs` (if missing) and a value type to `types.rs`.
2. Add a `Command` variant + encoding in `command.rs` (with a test using the
   bytes from the decompiled app), and implement `implied_update` /
   `confirmation_request`.
3. Decode it in `update.rs`, add a field to `DeviceState` in `state.rs`.
4. Add a capability flag to `DeviceProfile` and a control in the relevant
   GUI view (`crates/qcy-gui/src/views/`), CLI subcommand and/or tray entry
   (`crates/qcy-tray/src/menu.rs`).

### Add a platform

Implement `transport::Backend` (init / find / scan / open) and
`transport::Link` (read / write / notifications / disconnected / disconnect)
in a new module under `crates/qcy-client/src/backend/`, then select it in
`backend/mod.rs`. The `bluez` and `winrt` modules are complete examples of
about 500 lines each. Everything else (client, manager, GUI, CLI) works
unchanged; only the tray needs a renderer for the platform's tray API.

## Testing

* `cargo test --workspace`: protocol unit tests, many fed with bytes captured
  from real earbuds or copied from the decompiled app.
* Windows code is cross-compiled and its unit tests run under Wine:
  `CARGO_TARGET_X86_64_PC_WINDOWS_GNU_RUNNER=wine cargo test -p qcy-protocol
  -p qcy-client --target x86_64-pc-windows-gnu`. (Wine has no Bluetooth LE
  API, so the backend itself is only testable on real Windows.)
* `qcyctl -vv …` logs every read/write/notification in hex (`TRACE`).
* `qcy-gui --demo` runs against `Manager::spawn_demo()`: simulated earbuds
  built from `qcy_protocol::samples` (real captured data). Commands are
  encoded (so invalid ones still fail) and then applied to the state.
* `qcy-gui --screenshots DIR [--appearance light|dark]` renders every tab and
  a few interactive states (custom EQ editor, a dialog) once connected, saves
  them as PPM files, then exits. Combine with `--demo` to review UI changes
  without hardware; this also works for the Windows build under Wine.

[`Command`]: ../crates/qcy-protocol/src/command.rs
[`Update`]: ../crates/qcy-protocol/src/update.rs
[`Backend`, `Link`]: ../crates/qcy-client/src/transport.rs
