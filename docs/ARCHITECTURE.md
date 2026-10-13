# Architecture

```
┌──────────────┐  ┌───────────────────┐  ┌──────────────┐
│   orphion    │  │     orphion-tray      │  │    orphionctl    │   front-ends
│ egui window  │  │ KDE SNI / Windows │  │ command line │
└──────┬───────┘  └─────────┬─────────┘  └──────┬───────┘
       │ Manager            │ Manager           │ Earbuds (direct)
┌──────▼────────────────────▼───────────────────▼─────────┐
│ qcy-client                                               │   client
│  client    ─ Earbuds: runs the codec's GATT operations   │
│  manager   ─ background (re)connect, snapshots, demo     │
│  transport ─ Backend / Link traits, Endpoint, Selector   │
│  backend/  ─ bluez (Linux)  │  winrt (Windows)            │
└──────┬───────────────────────────────┬──────────────────┘
       │ Command / Op / Update          │
┌──────▼────────────────────────────┐   │
│ qcy-protocol (pure, no I/O)       │   │                     protocol
│  catalog · panel   what a model   │   │
│                    offers         │   │
│  dialect (qcy · jieli/rcsp · zr)  │   │
│                    how it's driven│   │
│  command · update · state · frame │   │
│  ids · types · eq · advert        │   │
└───────────────────────────────────┘   │
        Linux: D-Bus (zbus) → BlueZ ─┐  │  Windows: Windows.Devices.Bluetooth
                                     ▼  ▼
                BLE ── earbuds, services 0xA001 (+ 0xA002 on JieLi)
```

Two questions decide everything about a model, and each has its own module:

* **What does it offer?** Its *control panel* (`panel`, data from
  `catalog`): the same JSON document QCY's app downloads and builds its UI
  from, bundled for all 193 models. Front-ends render panels; there is no
  per-model code.
* **How is it driven?** Its *dialect* (`dialect`), chosen from the chip
  family: QCY's TLV protocol (WuQi, Realtek, generic), JieLi RCSP, or ZR.
  Front-ends never see the difference: they send the same `Command`s and
  read the same `DeviceState`.

## Crates

| Crate          | Responsibility |
|----------------|----------------|
| `qcy-protocol` | Turns typed [`Command`]s into GATT operations and bytes into typed [`Update`]s for every chip family (`dialect`), and describes every model (`catalog`, `panel`). No Bluetooth code at all. |
| `qcy-client`   | The stateful `Earbuds` client and the long-running `Manager`, written against two traits ([`Backend`, `Link`]). Implements them for BlueZ (`backend/bluez`, Linux) and WinRT (`backend/winrt`, Windows); `Platform` is the one for the build target. |
| `orphionctl`       | One-shot CLI on top of `Earbuds`. `controls.rs` addresses the panel's rows, modes and gestures by name. |
| `orphion`      | egui control panel on top of `Manager`. Its `material/` module is a small, app-independent Material 3 design system (colour roles, type scale, Material Symbols icons, cards, switches, chips, segmented buttons, navigation rail, snackbars, dialogs); `views/` render the panel; `widgets.rs` holds the app-specific drawings (battery rings, EQ curve). |
| `orphion-tray`     | Tray applet on top of `Manager`. `menu.rs` decides what is shown and done; `sni.rs` draws it as a KDE StatusNotifierItem (Linux), `win.rs` as a notification-area icon (Windows). |

### Inside `qcy-protocol`

| Module | Content |
|---|---|
| `catalog` | The bundled panels (`data/panels.json.zz`, 58 KB), model names, fallback panels |
| `panel` | Typed panels: `AncControl` (scene, legacy, percent), `EqControl` (graphic, parametric v1/v2), `KeyControl` (gesture table, per-gesture characteristics, buttons, head gestures) and the settings rows (`Item`) |
| `dialect` | `Codec`: what to read after connecting, what to poll, how to encode a command and confirm it, how to decode data, per dialect. `rcsp` (JieLi framing), `jieli` (QCY settings over RCSP), `zr` (EQ blob, key order) |
| `command`, `update`, `state` | The shared vocabulary: commands, decoded values, and the `DeviceState` they fold into (typed fields plus the raw payload of every setting and characteristic) |
| `frame`, `ids`, `types`, `eq`, `advert`, `uuids` | The QCY TLV protocol, value types, EQ maths, advertisement parsing, GATT identifiers |

## Data flow

1. **Discovery** (`Backend::find`): collects QCY advertisements (company id
   `0x521C`), parses them into `Advertisement`s and matches their embedded
   classic address to paired audio devices. BlueZ keeps a system-wide device
   cache; on Windows an advertisement watcher feeds the backend's own cache,
   persisted in `%LOCALAPPDATA%\orphion\endpoints.txt` so a second
   program can find already-connected earbuds (which stop advertising).
   `Endpoint::pick` ranks candidates: already-connected link, then the headset
   connected for audio, then paired, then any.
2. **Link** (`Backend::open`): connects without cancelling slow attempts
   (BlueZ: `Connect` + `Connected` watch; Windows: uncached service discovery
   with retries) and maps the characteristics of services `0xA001` and
   `0xA002` only. Writes use the characteristic's own write type (without
   response when supported), like Android does.
3. **Client** (`client.rs`): `Earbuds::connect` loads the model's panel,
   creates its `Codec`, subscribes to notifications and runs the codec's
   connect and refresh operations. Every value is decoded by the codec and
   folded into a `DeviceState` published through a `tokio::sync::watch`.
   `send(Command)` runs the codec's encoding, applies an optimistic update,
   and reads the setting back a second later (resets: re-reads everything
   after the earbuds settle). JieLi models are polled for battery.
4. **Manager** (`manager.rs`): owns the client in a background task, retries
   with backoff while the user wants to be connected (earbuds reboot after
   LDAC / dual-device changes and vanish into the case), and exposes
   `Snapshot`s (with the model's panel and the characteristics it has) plus
   `Notice`s for snackbars.
5. **Front-ends** render the panel and the state and push commands. The
   GUI's views are pure: they return `Action`s that `App` executes after the
   frame.

## Design rules

* **Protocol knowledge lives only in `qcy-protocol`.** Front-ends never build
  bytes; they use `Command`, `DeviceState` and the panel types. Panel rows
  that carry bytes (switches, resets) produce their commands themselves
  (`Switch::command`, `EqControl::apply_preset`, `panel::anc_command`).
* **No per-model code.** Model differences are data (the panel) or chip
  families (the dialect). A model the code has never seen works if its panel
  is in the catalogue.
* **Platform code lives only in `qcy-client/src/backend/`.** Everything above
  the `Backend` / `Link` traits is shared by all operating systems.
* **Unknown data is preserved.** Every setting's raw payload is kept in
  `DeviceState::settings`; unknown key actions and ANC values round-trip as
  `Other(..)`.
* **Writes are confirmed, not trusted.** The device is the source of truth;
  optimistic updates are corrected by its reply.
* **Never touch firmware.** Backends only resolve characteristics of the
  vendor services, so the OTA service is unreachable; panel rows for firmware
  update are dropped while parsing; the RCSP encoder can only build an
  allow-list of opcodes; key bytes are never written to the reset
  characteristic of non-JieLi models.

## Extending

### Support a new QCY model

Usually nothing to do: run `scripts/update-panels.py --fetch` to download
the current catalogue and panels, rebuild, and the model is supported with
the controls QCY's app shows for it. `orphionctl panel <model id>` prints what
it got.

If the model's panel uses a layout type this project does not parse yet,
`orphionctl panel` will not list it: add it to `panel/` (see
`research/notes/panel_semantics.md` for every known type), then render it in
the GUI view it belongs to.

### Add a setting the panels do not describe

1. Add its id to `ids.rs` (if missing) and a value type to `types.rs`.
2. Add a `Command` variant + encoding in `command.rs` (with a test using the
   bytes from the decompiled app), and implement `implied_update` /
   `confirmation_request`. Or use `Command::SetValue` if the payload is
   simple.
3. Decode it in `update.rs` and add a field to `DeviceState` in `state.rs`
   if a typed value helps (the raw payload is always in
   `DeviceState::settings`).
4. If JieLi or ZR models carry it differently, translate it in their dialect
   module.
5. Add a control to the GUI view, the CLI and/or the tray menu.

### Add a platform

Implement `transport::Backend` (init / find / scan / open) and
`transport::Link` (read / write / notifications / disconnected / disconnect)
in a new module under `crates/qcy-client/src/backend/`, then select it in
`backend/mod.rs`. The `bluez` and `winrt` modules are complete examples of
about 500 lines each. Everything else (client, manager, GUI, CLI) works
unchanged; only the tray needs a renderer for the platform's tray API.

## Testing

* `cargo test --workspace`: protocol unit tests, many fed with bytes captured
  from real earbuds or copied from the decompiled app; every bundled panel is
  parsed; JieLi and ZR encodings are checked against vectors from the app's
  code.
* `cargo run -p qcy-protocol --example panels [MODEL]` summarises what every
  model's panel offers (or one model in detail).
* Windows code is cross-compiled and its unit tests run under Wine:
  `CARGO_TARGET_X86_64_PC_WINDOWS_GNU_RUNNER=wine cargo test -p qcy-protocol
  -p qcy-client --target x86_64-pc-windows-gnu`. (Wine has no Bluetooth LE
  API, so the backend itself is only testable on real Windows.)
* `orphionctl -vv …` logs every read/write/notification in hex (`TRACE`).
* `orphion --demo [MODEL]` runs against `Manager::spawn_demo()`: simulated
  earbuds of any catalogue model (the MeloBuds Pro from real captured data).
  Commands go through the model's real codec, so invalid ones still fail,
  and are then applied to the state.
* `orphion --screenshots DIR [--appearance light|dark]` renders every tab and
  a few interactive states (custom EQ editor, a dialog) once connected, saves
  them as PPM files, then exits. Combine with `--demo MODEL` to review the UI
  of any model without hardware; this also works for the Windows build under
  Wine.

[`Command`]: ../crates/qcy-protocol/src/command.rs
[`Update`]: ../crates/qcy-protocol/src/update.rs
[`Backend`, `Link`]: ../crates/qcy-client/src/transport.rs
