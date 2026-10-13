# QCY earbuds control protocol

Reverse-engineered from the official **QCY** Android app v4.0.7
(`com.qcymall.earphonesetup`, protocol library `com.qcymall.qcylibrary`) and
verified against a **QCY MeloBuds Pro** (model id `0x4D4A`, WuQi chip,
firmware 2.0.6). Every example byte string in §1–§9 was captured from that
device unless stated otherwise. §10–§12 cover the per-model control panels
and the JieLi and ZR chips. They come from the app's code and public
sources and have not been checked against those earbuds yet.

Detailed write-ups with code references are in `research/notes/`:
`panel_semantics.md` (control panels), `jieli.md`, `zr.md` and
`web_research.md` (public sources and their licences).

> **Safety rule of this project:** firmware updates (OTA/DFU) are never
> performed or implemented. The OTA service `0x7033` is never touched: the
> transport only resolves characteristics of services `0xA001` and `0xA002`,
> and the JieLi encoder cannot express update, file transfer or reboot
> opcodes.

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
| `id & 0xF0 == 0x40`           | WuQi    | this protocol (§3–§9)                      |
| `id & 0xF0 == 0xC0`           | Realtek | this protocol (§3–§9)                      |
| `id & 0xF0 == 0x70`, `id==512`| JieLi   | JieLi RCSP on service `0xA002` (§11)       |
| `id & 0xF0 == 0x90`           | ZR      | this service with its own EQ/keys (§12)    |
| otherwise                     | generic | this protocol (§3–§9)                      |

Of the 193 earphones in QCY's catalogue, 95 are JieLi, 9 ZR, 25 WuQi,
9 Realtek and 55 generic. `orphionctl models` lists them.

## 3. GATT layout

Service **`0000a001-0000-1000-8000-00805f9b34fb`**:

| UUID (16-bit) | Properties              | Purpose                                         |
|---------------|-------------------------|-------------------------------------------------|
| `0x1001`      | write without response  | Framed commands                                 |
| `0x1002`      | read, notify            | Framed settings / replies                       |
| `0x0008`      | read, notify            | Battery                                         |
| `0x0007`      | read                    | Firmware versions                               |
| `0x000B`      | read, write w/o resp.   | Legacy graphic EQ (ZR: biquad blob, §12)        |
| `0x000D`      | read, write w/o resp.   | Touch key table                                 |

Older models (seen in their panels, not on the MeloBuds Pro) also use:

| UUID (16-bit)     | Purpose                                                     |
|-------------------|-------------------------------------------------------------|
| `0x0001`–`0x0006` | One touch gesture each (L/R single, double, triple): `[action]` |
| `0x0009`          | Voice prompt language (read: ASCII list; write: one code byte) |
| `0x000A`          | `01` restore defaults, `02` clear pairing, `03` factory reset |
| `0x000C`          | Clock (u32 BE Unix seconds)                                 |
| `0x000E`          | ZR status, clock and reset (§12)                            |
| `0x000F`          | `[in-ear, monitor]` switches: `01` on, `02` off             |

In the per-gesture key layout, the long press also uses `0x0009`/`0x000A`.
JieLi models map those to RCSP. On any other model a key byte written there
would reset the earbuds, so this project refuses to write it.

JieLi models add service **`0000a002-…`** with `0x0001` (write) and
`0x0002` (notify) for RCSP (§11).

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

Presets, band layout, Q values and `volumelevel` come from each model's
control panel (§10). Panels without `q` use the graphic EQ on `0x000B`;
panels with `q` but no `iitType` use command `0x20` (6-byte bands, no
filter type).

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

## 10. Control panels: per-model capabilities

The app has no per-model code. It downloads a "control panel" JSON for the
connected model and builds its whole UI from it:

```
POST https://api.watch.qcy.com/product/findControlPanels
Content-Type: application/x-www-form-urlencoded
modelId=19786&firmwareVersion=999.0.0
```

No account or token is required; `999.0.0` returns the newest panel. The
product list (`product/findProductList`) points to a zip with every model.
`scripts/update-panels.py --fetch` downloads both into `research/panel_json/`
and packs the panels into `crates/qcy-protocol/data/panels.json.zz`, which is
compiled in (`qcy_protocol::catalog`), so this project works offline. Models
without a panel use the app's built-in fallbacks (`controlpan_default.json`,
and `controlpan_jl.json` for JieLi).

A panel is a list of `layouts`, each with a `type`. Numbers may be strings
or floats (`"10"`, `103.0`), so read them like Android's `optInt`.

| type | content | mapped to |
|---|---|---|
| 1 | gestures, one characteristic each (`character` + hex `cmd`) | `KeyControl` (`Panel::legacy_keys`) |
| 2 | EQ: `freq`, `q`, `iitType`, `maxdb`, `basedb`, `volumelevel`, presets in `sys_eq` | `EqControl` |
| 3 | "Find headset" (phone side only) | ignored |
| 4 | settings rows: switches (item types 200–206, `sub_items` with the exact bytes), buttons (resets), fit test (301); item type 1 is firmware update | `Item` (firmware update skipped) |
| 6 | legacy ANC buttons, `cmdid` = value of command `0x0C` | `AncControl::Legacy` |
| 8 | left/right balance (command `0x16`) | `Panel::balance` |
| 9 | ANC modes with scenes and levels as packed ranges (`startcmdid`…`endcmdid`, `mode << 16 \| scene << 8 \| level`), `voicecmdid` for voice focus | `AncControl::Scenes` |
| 10 | touch gestures: `music.event[]` with `eventID` and action lists; fixed call gestures in `voice` | `KeyControl` (`Panel::touch_keys`, key table `0x000D`) |
| 11 | ANC modes with a strength in percent (JieLi) | `AncControl::Percent` |
| 14 | headphone buttons | `Panel::buttons` |
| 20 | earbud volume (command `0x08`) | `ItemKind::Volume` |
| 23 | game EQ presets (graphic) | `Panel::game_eq` |
| 101 / 102 / 103 | rename / sleep timer (`0x14`) / prompt tone volume (`0x1D`, `itemName` maps level → label) | `ItemKind` |
| 104 | wear detection (`0x2C`) with music options and sensitivity (`0x48`) | `ItemKind::WearDetection` |
| 110 | fit test (`0x11`: write `01 01 01`, result `[2, L, R]`, 3 = good seal) | `ItemKind::FitTest` |
| 111 / 116 | sound mode (`0x2E`) / spatial audio mode (`0x5B`), by index | `ItemKind::Choice` |
| 112 | game mode + game EQ sync (`0x4A`: `[enabled, sync]`, restarts) | `ItemKind::GameConfig` |
| 113 | multipoint (`0x24`) and paired phones (`0x4D`) | `ItemKind::Multipoint` |
| 114 | head gestures: enable with `0x50`, actions `0x51 [event, action]` | `ItemKind::HeadGestures` |
| 115 | AI noise cancelling on/off (`0x54`) | `ItemKind::AiAnc` |
| 1000 | any setting from a list of hex values (`itemName` / `subItem`), e.g. auto power-off `0x1F` | `ItemKind::Choice` |
| 1001 | prompt language (`itemKey` = hex of the ASCII code, e.g. `WQ00`) | `ItemKind::Language` |

Settings without a name in the app (anti-wind `0x2A`, adaptive volume
`0x40`, eco mode `0x53`, bass boost `0x57`, …) work purely through these
generic rows. The panels contain some quirks; this is how this project
handles them:

* Type 206 "Gaming mode" items carry the sleep mode bytes (`FF0310…`), and
  the app really toggles sleep mode on non-JieLi models. This project sends
  the gaming mode command (`0x09`) instead.
* JieLi panels give switch bytes without the frame (`230101`). They are
  framed (`FF 03 23 01 01`) before use.
* Sleep timer `0` powers the earbuds off at once; "off" is `0xFFFF`.
* Frequencies like `1K` are parsed as 1000 Hz (the app drops them).

## 11. JieLi models (RCSP)

QCY's JieLi models use JieLi's small `com.jieli.jl_bt_rcsp` library. Control
is BLE only, on QCY's own service `0xA002` (write `0x0001`, notify `0x0002`),
with **no authentication**. The JieLi E1/SAFER+ handshake that other JieLi
products use is documented in `research/notes/jieli.md` §3 but is not needed.

Frames (`dialect::rcsp`):

```
FE DC BA | flags | opcode | len (BE16) | body | EF
flags: 0xC0 command wanting a response, 0x80 command, 0x00 response
body:  command: sn, params…   response: status, sn, params…
```

There is no checksum, and notifications may split frames. On connect the app
sends target info `03` (mask `FF FF FF FF`, platform 0) and then the clock
(`C0`, attribute 7 = u32 BE seconds). JieLi earbuds do not notify battery
changes, so the app polls every 30 s. Models that cannot notify ANC changes
are polled every 3 s, battery and ANC together.

* **QCY settings** travel in the custom opcode `FF` as one TLV, without the
  `FF len` header: `FE DC BA C0 FF 00 06 sn 17 03 01 01 02 EF` sets ANC
  (1, 1, 2), and `… FF 00 04 sn FE 01 2F EF` reads the battery.
* **Older models** (`id & 0x80F0 != 0x8070`) use "ADV" attributes instead:
  `C0` sets one LTV record `len type data`, and `C1` gets attributes by a
  BE32 mask. The attributes are:
  * 0: battery
  * 1: name
  * 2: keys, as `[key, clicks, function]` (key 1 = left, 2 = right; "no
    action" is sent as 127)
  * 5: work mode (2 = game)
  * 10: sleep mode
  * 11: noise mode with a percent strength
  * 12: balance

  `C5 01` and `C5 02` restore defaults and clear pairing.
* **Every JieLi model** uses attribute 8 for in-ear detection. The graphic EQ
  goes through system info (`07`/`08`, function `FF`, type 4: `mode, g0…g9`;
  presets are sent with every gain set to 127). Parametric EQs use the custom
  command with at most 10 bands.

Opcodes this project never builds:

* firmware update: `E1`–`E6`, `E8`
* reboot: `E7`
* MD5: `D4`
* data and file transfer: `01`, `16`–`1E`, `22`–`28`
* system operation 3: `C5 03`

## 12. ZR models

ZR earbuds use service `0xA001`, with these differences:

* **EQ** on `0x000B` is a 144-byte blob, in this order:
  * five RBJ peaking biquads at 62, 250, 1000, 4000 and 8000 Hz (Q 1.5,
    1.5, 3, 3, 3), as Q2.21 int32 LE coefficients
  * the preset id
  * the gains
  * `5A 5A`
  * the frequencies
  * a checksum: the sum of all bytes except the preset id

  Preset 1 is always flat. Before the first EQ write of a session the app
  sends the clock to `0x000E` (`00 00` + u32 BE). Reading `0x000B` returns
  the blob, with the preset at byte 120.
* **Keys** on `0x000D` number the gestures 1–3 for the left bud and 4–6 for
  the right (QCY uses odd = left, even = right). The table is always written
  whole.
* **Status** is on `0x000E`: byte 1 & 0x7F == 1 means in-ear detection is on.
* Resets go to `0x000E` or `0x000A`, whichever the model's panel names.

`dialect::zr` reproduces the app's blob byte for byte; its tests use vectors
computed from the app's code.
