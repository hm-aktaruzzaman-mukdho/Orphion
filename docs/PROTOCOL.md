# QCY earbuds control protocol

Reverse-engineered from the official **QCY** Android app v4.0.7
(`com.qcymall.earphonesetup`, protocol library `com.qcymall.qcylibrary`) and
verified against a **QCY MeloBuds Pro** (model id `0x4D4A`, WuQi chip,
firmware 2.0.6). Every example byte string below was captured from that
device unless stated otherwise.

> **Safety rule of this project:** firmware updates (OTA/DFU) are never
> performed or implemented. The OTA service `0x7033` is never touched; the
> transport only resolves characteristics of service `0xA001`.

---

## 1. Two Bluetooth identities

A QCY headset shows up as **two** devices:

| Role    | Transport | Example address      | Notes                                   |
|---------|-----------|----------------------|-----------------------------------------|
| Audio   | BR/EDR    | `84:AC:60:89:A4:B0`  | Paired normally; A2DP, HFP, AVRCP, SPP  |
| Control | BLE       | `C4:AC:60:89:A4:B1`  | Random address, never paired/bonded     |

The phone app scans for BLE advertisements, matches them to the audio device
and opens a GATT connection to the BLE identity. No pairing, bonding or
authentication handshake is involved. Any central may connect and write.

Connected peripherals stop advertising, so only one central (phone app *or*
this project) can usually hold the control link at a time. BlueZ shares one
link between all local D-Bus clients, though.

## 2. Advertisement

Manufacturer specific data, company id **`0x521C`** (`BuildConfig.CompanyID =
21020`; the app also accepts `0x522C`). At least 20 bytes. Parsed by
`Devicebind.refreshInfoFromAVData`:

| Offset  | Meaning                                                            |
|---------|--------------------------------------------------------------------|
| 0–1     | Model id, big endian (`4d 4a` = 19786 = MeloBuds Pro)               |
| 3       | bits 1–3: colour index                                              |
| 5, 6, 7 | Battery left / right / case: bit 7 = charging, bits 0–6 = percent   |
| 11–16   | Classic address of this bud, bytes in order `12 11 13 16 15 14`     |
| 18–23   | Classic address of the other bud, order `19 18 20 23 22 21` (0 = same) |

Captured: `4d 4a ad 80 6e 5e 5c 00 5c 9a 0f ac 84 60 b0 a4 89 02 ac 84 60 2b a5 89`
→ model `0x4D4A`, L 94 %, R 92 %, audio `84:AC:60:89:A4:B0`, peer `84:AC:60:89:A5:2B`.

### Chip families

The app routes models to different code paths by model id
(`Devicebind.is*Device`):

| Condition                     | Family  | Protocol                                   |
|-------------------------------|---------|--------------------------------------------|
| `id & 0xF0 == 0x40`           | WuQi    | **this protocol**                          |
| `id & 0xF0 == 0xC0`           | Realtek | **this protocol**                          |
| `id & 0xF0 == 0x70`, `id==512`| JieLi   | JieLi RCSP SDK (not implemented)           |
| `id & 0xF0 == 0x90`           | ZR      | own GATT layout (not implemented)          |
| otherwise                     | generic | **this protocol**                          |

## 3. GATT layout

Service **`0000a001-0000-1000-8000-00805f9b34fb`**:

| UUID (16-bit) | Properties              | Purpose                                         |
|---------------|-------------------------|-------------------------------------------------|
| `0x1001`      | write without response  | Framed commands                                 |
| `0x1002`      | read, notify            | Framed settings / replies                       |
| `0x0008`      | read, notify            | Battery                                         |
| `0x0007`      | read                    | Firmware versions                               |
| `0x000B`      | read, write w/o resp.   | Legacy graphic EQ                               |
| `0x000D`      | read, write w/o resp.   | Touch key table                                 |

The device also exposes service `0x7033` (WuQi OTA) with characteristics that
**reuse the UUIDs `0x1001`/`0x1002`**. Characteristics must therefore be
looked up by their parent service.

The app enables notifications on every notifying characteristic, negotiates
the MTU (220 on the MeloBuds Pro under BlueZ), and never fragments: each
command is a single ATT write and must fit into `MTU − 3` bytes.

## 4. Framing (`0x1001` / `0x1002`)

```
FF  len  { id  n  payload[n] } …
```

`len` counts the bytes after itself. A frame may carry several entries; the
settings dump on `0x1002` carries all of them at once. Builders and parser:
`DataAnalyse.getSendData` / `DataAnalyse.analyseAllCMD`.

To read a setting, write `FF 03 FE 01 <id>` ("request data"). The reply is
notified on `0x1002`. Reading `0x1002` returns a dump of most settings:

```
ff 30 | 2c 04 00 01 01 01 | 08 03 46 46 0f | 09 01 02 | 17 03 01 05 00 | 10 01 00
      | 16 01 32 | 04 01 02 | 23 01 00 | 24 01 01 | 1f 02 05 00 | 1d 02 08 0f
      | 19 04 57 51 30 30
```

### Boolean settings

On/off settings are written as `1` = on, `2` = off and read back as `1` = on
(anything else = off). E.g. gaming mode on: `FF 03 09 01 01`.

## 5. Settings reference

| Id   | Name (app constant)          | Payload (write → read)                                  |
|------|------------------------------|---------------------------------------------------------|
| 0x01 | `RESET_DEFAULT`              | empty: restore settings (`FF 02 01 00`)                 |
| 0x02 | `CLEAR_PAIR`                 | empty: forget phones                                    |
| 0x03 | `FACTORY_RESET`              | empty (`FF 02 03 00`)                                   |
| 0x04 | `MUSICACTION`                | write 1 play, 2 pause, 3 prev, 4 next; read 1 = playing |
| 0x08 | `VOICE` (media volume)       | `[left, right, 0]` → `[left, right, max]`               |
| 0x09 | `DIYANSHI` (gaming mode)     | bool                                                    |
| 0x10 | `SLEEPMODE`                  | bool (touch disabled until back in case)                |
| 0x14 | `POWERMANAGER` (sleep timer) | `[min_lo, min_hi, 0, 0]` → `[min_lo, min_hi, left_lo, left_hi]`, `FFFF` = off. **Minutes `0` powers the earbuds off at once** (the app's "Shut down" button); never send it to cancel |
| 0x16 | `BALANCE`                    | `0..=100`, 50 = centre                                  |
| 0x17 | `ANCSETTING`                 | `[mode, scene, level]`, see §6                          |
| 0x18 | `PAIRNAME`                   | UTF-8 name ≤ 32 bytes; read is NUL padded               |
| 0x19 | `VOICENAME` (prompt language)| ASCII code: `WQ00` English, `WQ01` Chinese              |
| 0x1D | `TONEVOLUME`                 | write `[level, 0]` → `[level, max]` (app: 4/6/8/10 of 15); vendor `0x4161` only takes `[level]` |
| 0x1F | `STANDBY` (auto power off)   | u16 LE minutes after disconnect, `FFFF` = never         |
| 0x20 | `MULTIEQ`                    | parametric EQ, 6-byte bands (older models)              |
| 0x22 | `MULTIEQ2`                   | parametric EQ, 7-byte bands, see §7                     |
| 0x23 | `LDAC`                       | bool; enabling reboots and disables dual device         |
| 0x24 | `MOREDEVICE` (dual device)   | bool; enabling reboots and disables LDAC                |
| 0x2B | `KEYFUN`                     | key table inside a frame (same format as `0x000D`)      |
| 0x2C | `PEIDAI` (wear detection)    | write `[enabled, music, anc]` → read `[enabled, music, anc, tone]` |
| 0x44 | `MAXEQ_COUNT`                | max parametric bands (no reply on MeloBuds Pro → 20)    |
| 0xFE | `REQUESTDATA`                | `[id]`: ask for a setting                               |

Wear detection `music`: 0 nothing, 1 pause and resume, 2 pause only,
3 pause and resume only when both are worn. `anc` = 1 turns ANC off when
removed and back on when worn.

Many more ids exist (spatial audio, LED, alarms, AI ANC, hearing health…);
see `CommandId` in `crates/qcy-protocol/src/ids.rs`. Unknown ids are kept as
raw bytes in `DeviceState::unknown`.

## 6. Noise control (`0x17`)

`[mode, scene, level]`. The control panel JSON packs it as
`mode << 16 | scene << 8 | level` (`startcmdid` / `endcmdid` ranges).

| Mode | Scene | Level | Meaning (MeloBuds Pro)                    | Packed        |
|------|-------|-------|-------------------------------------------|---------------|
| 1    | 1     | 0–2   | ANC, Indoor                               | `0x010100–02` |
| 1    | 2     | 0–2   | ANC, Commuting                            | `0x010200–02` |
| 1    | 3     | 0–2   | ANC, Noisy                                | `0x010300–02` |
| 1    | 4     | 0     | ANC, Anti-wind                            | `0x010400`    |
| 1    | 5     | 0     | ANC, Adaptive                             | `0x010500`    |
| 2    | 0     | 0     | Off ("Normal")                            | `0x020000`    |
| 3    | 1     | 1–6   | Transparency, strength                    | `0x030101–06` |
| 3    | 2     | 0     | Transparency, voice focus                 | (`0x030100`*) |

\* The panel declares voice focus as `0x030100`, and `QCYHeadsetClient.setNoiseMode`
rewrites exactly that value to `(3, 2, 0)` before sending.

The earbuds apply a mode change only after ~0.5 s. A read issued sooner
returns the previous value, so the client confirms changes after one second.

## 7. Equalizer

### Graphic EQ: characteristic `0x000B`

`[preset, g0 … g9]`, gains as signed dB. On current firmwares it still
reports the active preset id (`01 00 … 00` = preset 1). Older models are set
by writing to it.

### Parametric EQ: `0x22` (`MULTIEQ2`)

```
index, master_lo, master_hi, { f_lo f_hi  g_lo g_hi  q_lo q_hi  type } × n
```

* `index`: preset id (1–10), or 255 for a custom curve
* `master`: i16 LE, dB × 100 (truncated); typically ≤ 0
* `f`: Hz; `g`: i16 LE dB × 100 (rounded); `q`: u16 LE × 100 (rounded)
* `type`: filter type from the panel's `iitType` (0 = peaking; presets use 4
  for their 30 Hz band). Padding bands: `f = 0, g = 0, q = 0, type = 255`.

The app pads the list to `min((MTU−3−7)/6, MAXEQ_COUNT or 20)` bands. The
MeloBuds Pro stores 20 bands: reading `0x22` returns `ff 91 22 8f …`, which is
143 = 3 + 20 × 7 bytes.

The custom 10-band editor uses centre frequencies 31, 62, 125, 250, 500, 1k,
2k, 4k, 8k and 16k Hz, Q = 1, gains −8…+8 dB. Its master gain is computed by
`QCYConnectManager.getMaxGain`: sum the RBJ peaking filter responses
(fs = 44.1 kHz) from 20 Hz to 20 kHz in 5 Hz steps, add 0.5 dB, and if the
peak exceeds `volumelevel` (8 dB) below 1.5 kHz, or `volumelevel + 4` above
it, lower the master gain by the excess. Ported in `eq::auto_master_gain`.

MeloBuds Pro presets (`sys_eq` of the panel JSON) are transcribed in
`crates/qcy-protocol/src/profile.rs`.

## 8. Touch keys (`0x000D`)

Pairs of `[gesture, action]`, zero-padded:
`01 00 02 00 03 01 04 01 05 04 06 03 07 00 08 00 00 00 00 00`.
A write may contain just the pairs that change (`04 03` = right double tap →
next track).

Gestures (odd = left, even = right): 1/2 single, 3/4 double, 5/6 triple,
7/8 quadruple, 9/10 long press.

Actions (MeloBuds Pro panel): 0 none, 1 play/pause, 2 previous, 3 next,
4 voice assistant, 5 volume up, 6 volume down, 7 gaming mode, 11 cycle ANC.

## 9. Battery (`0x0008`) and version (`0x0007`)

Battery: three bytes, left / right / case, bit 7 = charging, 0 = unknown
(`41 4b 00` = 65 %, 75 %, case unknown). Notified on change.

Version: `[L.major, L.minor, L.patch, R.major, R.minor, R.patch]`
(`02 00 06 02 00 06` = 2.0.6 / 2.0.6).

## 10. Per-model capabilities

The app downloads a "control panel" JSON per model:

```
POST https://api.watch.qcy.com/product/findControlPanels
Content-Type: application/x-www-form-urlencoded
lang: en
sys_: android
modelId=19786&firmwareVersion=2.0.6
```

No account or token is required. The MeloBuds Pro document is saved in
`research/panel_json/`. Its `layouts` describe key options (`type 10`), the
EQ (`type 2`), ANC modes (`type 9`), and simple items by `cmdid` (wear
detection 44, prompt tone 29, language 25, auto power off 31, toggles…). The
relevant parts are transcribed into `profile.rs`, so this project needs no
network access.
