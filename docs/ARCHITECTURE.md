# Architecture

```
┌──────────────┐  ┌──────────────┐  ┌──────────────┐
│   qcy-gui    │  │   qcy-tray   │  │    qcyctl    │   front-ends
│ egui window  │  │ KDE/SNI tray │  │ command line │
└──────┬───────┘  └──────┬───────┘  └──────┬───────┘
       │ Manager         │ Manager         │ Earbuds (direct)
┌──────▼─────────────────▼─────────────────▼───────┐
│ qcy-bluez                                         │   transport + client
│  discovery ─ find the BLE endpoint of a headset   │
│  gatt      ─ connect, read/write, notifications   │
│  client    ─ Earbuds: live DeviceState + commands │
│  manager   ─ background (re)connect, snapshots    │
└──────┬────────────────────────────────────────────┘
       │ bytes
┌──────▼────────────────────────────────────────────┐
│ qcy-protocol   (pure, no I/O, unit tested)        │   protocol
│  frame · ids · types · eq · command · update      │
│  state · advert · profile                         │
└───────────────────────────────────────────────────┘
       │ D-Bus (zbus, pure Rust)
   BlueZ (bluetoothd)  ──BLE──  earbuds, service 0xA001
```

## Crates

| Crate          | Responsibility                                                       |
|----------------|----------------------------------------------------------------------|
| `qcy-protocol` | Turn typed [`Command`]s into bytes and bytes into typed [`Update`]s. Holds model profiles. No Bluetooth code at all. |
| `qcy-bluez`    | Everything BlueZ-specific: discovery, the GATT session, the stateful `Earbuds` client and the long-running `Manager`. |
| `qcyctl`       | One-shot CLI on top of `Earbuds`. Good for scripts and KDE shortcuts. |
| `qcy-gui`      | egui control panel on top of `Manager`.                              |
| `qcy-tray`     | StatusNotifierItem tray applet on top of `Manager`.                  |

## Data flow

1. **Discovery** (`discovery.rs`): `Bluez::find` reads BlueZ's object tree,
   scans (LE only) if needed, and parses QCY manufacturer data into an
   `Advertisement`. Candidates are ranked: already-connected link, then the
   headset currently connected for audio, then paired, then any.
2. **Session** (`gatt.rs`): `GattSession::open` connects without cancelling
   slow attempts, waits for service resolution, and maps the characteristics
   of service `0xA001` only.
3. **Client** (`client.rs`): `Earbuds::connect` subscribes to notifications,
   decodes every value with `qcy_protocol::update::decode`, and folds the
   results into a `DeviceState` published through a `tokio::sync::watch`.
   `send(Command)` serialises writes, applies an optimistic update, and asks
   the earbuds to confirm one second later.
4. **Manager** (`manager.rs`): owns the client in a background task, retries
   with backoff while the user wants to be connected (earbuds reboot after
   LDAC / dual-device changes and vanish into the case), and exposes
   `Snapshot`s plus `Notice`s for toasts.
5. **Front-ends** render snapshots and push commands. The GUI's views are
   pure: they return `Action`s that `App` executes after the frame.

## Design rules

* **Protocol knowledge lives only in `qcy-protocol`.** Front-ends never build
  bytes; they use `Command`, `DeviceState` and `DeviceProfile`.
* **Unknown data is preserved.** Unknown setting ids, key actions, ANC
  triples and wear behaviours round-trip as `Other(..)` / `Unknown { .. }`
  instead of being dropped.
* **Writes are confirmed, not trusted.** The device is the source of truth;
  optimistic updates are corrected by its reply.
* **Never touch firmware.** Only characteristics of service `0xA001` are
  resolved; the OTA service is unreachable by construction.

## Extending

### Support another QCY model

1. Find its model id: `qcyctl scan` prints `model=0x…`.
2. Fetch its control-panel JSON (see `docs/PROTOCOL.md` §10) and save it
   under `research/panel_json/`.
3. Add a `DeviceProfile` constant in `crates/qcy-protocol/src/profile.rs`
   (copy `MELOBUDS_PRO`) and list it in `PROFILES`.
4. If the model is JieLi- or ZR-based (`ChipFamily`), it needs a different
   protocol implementation; see "Add a transport" below.

Until a profile exists the model falls back to `GENERIC`, and the GUI still
shows any toggle the earbuds report on their own.

### Add a setting

1. Add its id to `ids.rs` (if missing) and a value type to `types.rs`.
2. Add a `Command` variant + encoding in `command.rs` (with a test using the
   bytes from the decompiled app), and implement `implied_update` /
   `confirmation_request`.
3. Decode it in `update.rs`, add a field to `DeviceState` in `state.rs`.
4. Add a capability flag to `DeviceProfile` and a control in the relevant
   GUI view (`crates/qcy-gui/src/views/`), CLI subcommand and/or tray item.

### Add a transport

`qcy-protocol` is transport-agnostic: it only needs "write these bytes to
characteristic X" and "bytes arrived from characteristic Y". A new transport
(another OS, a mock for tests, a JieLi bridge) re-implements `gatt.rs` and
reuses `client.rs`'s logic.

## Testing

* `cargo test --workspace`: protocol unit tests, many fed with bytes captured
  from real earbuds or copied from the decompiled app.
* `qcyctl -vv …` logs every read/write/notification in hex (`TRACE`).
* `qcy-gui --demo` runs against `Manager::spawn_demo()`: simulated earbuds
  built from `qcy_protocol::samples` (real captured data). Commands are
  encoded (so invalid ones still fail) and then applied to the state.
* `qcy-gui --screenshots DIR [--appearance light|dark]` renders every tab and
  a few interactive states (custom EQ editor, a dialog) once connected, saves
  them as PPM files, then exits. Combine with `--demo` to review UI changes
  without hardware.

[`Command`]: ../crates/qcy-protocol/src/command.rs
[`Update`]: ../crates/qcy-protocol/src/update.rs
