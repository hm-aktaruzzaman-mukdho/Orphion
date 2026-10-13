# ZR-chip QCY earbuds: protocol notes

Source: the official QCY Android app v4.0.7, decompiled with jadx (`research/decompiled/sources/`).
Unless stated otherwise, paths below are relative to `research/decompiled/sources/com/qcymall/`.
Nothing here was checked against real ZR hardware. Statements marked **[inferred]** or
**[unknown]** are guesses or gaps, not code facts.

Firmware update (OTA/DFU) is out of scope. The panels contain a "Firmware update"/"Firmware
version" item (`type 1`); it is ignored here and must never be implemented.

---

## 0. Which devices are ZR

`Devicebind.isZRDevice()` returns `(vendorId & 0xF0) == 0x90`
(`earphonesetup/model/Devicebind.java:960-962`). The vendor id is the big-endian model id at
advertisement bytes 0–1, as for every other QCY family (see `docs/PROTOCOL.md` §2). The
advertisement parser has no ZR special case.

ZR models in `research/panel_json/catalog/en_prod_0.0.0.2.5.json` (all `earphones`, none hidden):

| vendorId | hex      | Title  | Panel JSON (`research/panel_json/models/`) | Panel family (§4) |
|----------|----------|--------|--------------------------------------------|-------------------|
| 19857    | `0x4D91` | T9     | `19857.json`                               | B (10-band, 0x000A reset) |
| 19858    | `0x4D92` | T7     | `19858.json`                               | B + in-ear sensor |
| 19859    | `0x4D93` | T8     | `19859.json`                               | B + in-ear sensor |
| 19860    | `0x4D94` | T1C    | `19860.json`                               | A (5-band, 0x000E reset) |
| 19861    | `0x4D95` | M10    | `19861.json`                               | A |
| 19862    | `0x4D96` | T1S    | `19862.json`                               | B |
| 19863    | `0x4D97` | T2C    | `19863.json`                               | A |
| 19865    | `0x4D99` | QT12S  | `19865.json`                               | C (V2 key table, 0x1001 reset) |
| 19866    | `0x4D9A` | T13    | `19866.json`                               | B + sleep mode |

`0x4D98` (19864) is not in the catalog. It still appears in `Devicebind.checkBatIDS`
(`Devicebind.java:47`), together with all nine ids above.

The panel files were fetched on 2026-10-13 (see `research/logs/panel_fetch_jl_zr.log`, "ZR … layouts 4").
Every one reports `firmwareVersions 999.0.0`, range `0.0.0`–`999.999.999`, so one panel covers all
firmware versions.

---

## 1. GATT layout

ZR devices use the **same service as every other QCY family: `0000A001-0000-1000-8000-00805f9b34fb`**.
`QCYHeadsetClient` resolves every characteristic under that service only
(`qcylibrary/QCYHeadsetClient.java:137`, `qcylibrary/request/BLERequest.java:9`). There is no ZR
service UUID. What differs is which characteristics are used and what their payloads are.

### Connection behaviour (all families, ZR included)

`QCYHeadsetClient.connectDevice` (`QCYHeadsetClient.java:252-283`):

1. Discovers services and takes `service(0xA001).getCharacters()`.
2. **Enables notifications on every characteristic that has the NOTIFY property (`property & 0x10`)**.
3. Stores the list of characteristic UUIDs found (`deviceSuportCharacter`). Every later
   `readBleData`/`sendBleData` to a UUID that is **not** in this list is silently dropped and
   returns `false` (`QCYHeadsetClient.java:742-786`). Many features rely on this "try it,
   fall back if missing" behaviour.
4. Requests MTU 512 (`com/inuker/bluetooth/library/connect/request/BleChangeMTURequest.java:30`).
   The usable payload is stored as `mtu − 3`, minimum 20.

Writes go through inuker's `write()`, which uses the characteristic's default write type: write
request if the WRITE property is present, otherwise write-without-response
(`qcylibrary/request/BLEWriteRequest.java`). The app never fragments. On Android a write
request longer than `MTU−3` becomes a long (prepared) write automatically.

### Characteristics used for ZR

All UUIDs are `0000XXXX-0000-1000-8000-00805f9b34fb`. Constants are in
`earphonesetup/model/ControlerPanl.java:24-42` and `qcylibrary/QCYHeadsetClient.java:75-80`.

| UUID   | App constant | ZR use | Direction | Notes |
|--------|--------------|--------|-----------|-------|
| `0x0007` | `CHARACTER_VERSION_UUID` | firmware version | read | Standard, unchanged |
| `0x0008` | `CHARACTER_BATTERY_UUID` | battery | read + notify | Standard, unchanged |
| `0x000B` | `UUID_EQ` | **ZR EQ blob (144 bytes)** | read, write | ZR-specific payload, §3.1 |
| `0x000D` | `UUID_KEY_FUNCTION_V2` | key table | read, write | Standard pair format, **ZR key-id order**, §3.2 |
| `0x000E` | `UUID_SETTING_ZR_V1` | ZR "setting": status read, time sync, reset | read, write | §2.3, §3.3, §3.4 |
| `0x000F` | `UUID_FUNCTION_V1` | in-ear sensor on/off (T7, T8 panels) | write | §3.5 |
| `0x000A` | `UUID_RESET_V1` | reset / clear pairing (T9, T7, T8, T1S, T13 panels) | write | §3.4 |
| `0x000C` | `UUID_SENDTIME_V1` | time sync (tried after `0x000E`) | write | §3.3 |
| `0x1001` | `UUID_SETTING_V2` | TLV frames (T13 sleep mode, QT12S resets, request-data) | write | §5 |
| `0x1002` | `UUID_SETTING_READ_V2` | TLV replies, if present | read + notify | §5 |
| `0x0001`–`0x0006` | `UUID_KEY_FUNCTION_C{L,R}{1,2,3}_V1` | key reads, only as a fallback when `0x000D` is missing | read | §2.4 |
| `0x0009` | `UUID_LANUAGE` | prompt language; only read if the panel has a voice list, and no ZR panel has one | read | not used |

**[unknown] Real properties and presence.** No GATT dump of a ZR device exists in this
repository. The table lists what the app *uses*. Several entries are only tried (`0x000C`,
`0x1001`, `0x000A`, `0x000F`, `0x0001`–`0x0006`). On real hardware, enumerate
service `0xA001` first and branch on what exists, as the app does. The key-function write
path only works if `0x000D` exists (§3.2), so ZR firmware almost certainly has it **[inferred]**.

A Rust client should enable notifications on every notifying characteristic of `0xA001`,
request a large MTU (≥ 147 so a 144-byte EQ write fits in one ATT PDU), and look up
characteristics by parent service (`docs/PROTOCOL.md` §3). Never touch `0x7033`.

---

## 2. Reads

### 2.1 Battery: `0x0008` (unchanged)

`QCYHeadsetClient.getBattery` → `analyseBatteryData` (`QCYHeadsetClient.java:351-358, 656-672`),
parser `qcylibrary/dataBean/BatteryDataBean.java`. The format is `[left, right, case]`. In each
byte, bit 7 is the charging flag and bits 0–6 are the percentage. It is also delivered by
notification (`QCYHeadsetClient.java:810-812`).

ZR special case (UI only): every ZR id is in `Devicebind.checkBatIDS`, so the app shows
1–99 % **rounded to the nearest 10 %** (`Devicebind.java:645-651`:
`Math.round(v / 10f) * 10`). This hints that ZR firmware reports coarse battery values
**[inferred]**.

### 2.2 Version: `0x0007` (unchanged)

`QCYConnectManager.readVersion` has no ZR branch → `QCYHeadsetClient.getVersion`
(`QCYConnectManager.java:1305-1317`, `QCYHeadsetClient.java:364-374`, parser `VersionDataBean`):
`[L.maj, L.min, L.patch, R.maj, R.min, R.patch]`.

### 2.3 Settings / in-ear status: `0x000E`

`QCYConnectManager.readSettingStatus`, ZR branch (`QCYConnectManager.java:859-870`):

```java
readBleData(mac, UUID_SETTING_ZR_V1, (…, bArr) -> {
    if (bArr == null || bArr.length <= 2 || zrDevice == null) return;   // needs >= 3 bytes
    zrDevice.setCheck((bArr[1] & 0x7F) == 1);                            // in-ear detection on?
    SP["<mac>_BOOL_6"] = isCheck;
    EventBus(51 /* EVENT_BLE_ON_NOTIFY_RUER = "in-ear" */, mac, 6, isCheck);
});
```

| Byte | Meaning |
|------|---------|
| 0    | **[unknown]**, ignored by the app |
| 1    | bits 0–6 `== 1` → in-ear detection enabled. Bit 7 is masked off **[unknown meaning]** |
| 2…   | **[unknown]**. The app only requires length ≥ 3 |

Example: a read of `00 01 xx …` means in-ear detection is on, and `00 02 xx …` means it is off
(assuming the device uses the same 1 = on / 2 = off encoding as the `0x000F` write in §3.5;
**[inferred]**).

Id 6 is the app-wide id for in-ear detection (panel types 200/201 → `cmdid 6`,
`ControlerPanl.java:354-371`). Event 51 is `EventBusMessage.EVENT_BLE_ON_NOTIFY_RUER`
(`earphonesetup/model/EventBusMessage.java:53`), and **nothing in the app subscribes to it**.
The switch widget reads `"<mac>_SINGLEVALUE_6"` instead (`v2ui/view/SingleSwitchItemView.java`,
`getValueShow`), which for ZR is only set locally when the user toggles. So the app never
shows the device's in-ear state for ZR. The Rust client can do better by decoding byte 1.

Non-ZR devices read `0x000F` (+ `0x1002`) instead (`QCYConnectManager.java:871-897`).

### 2.4 Key functions: `0x000D` (pair format, ZR key ids)

`QCYConnectManager.readKeyFunction` (`QCYConnectManager.java:899-963`) has no ZR branch:

* if `0x000D` exists → `QCYHeadsetClient.readButtonFunction` (`QCYHeadsetClient.java:318-327`),
  which parses `[id, fn]` pairs with `KeyFuncDataBean` (`qcylibrary/dataBean/KeyFuncDataBean.java`);
* otherwise it reads `0x0001`…`0x0006` (one byte each = function of that key) **[fallback;
  the ZR write path cannot work in that case]**.

The listener then converts ids for ZR (`QCYConnectManager.java:1975-1986`):
`map = zrDevice.setKeyFunctionInfo(map)`, which renames every key id **from ZR numbering to QCY
numbering** with `converCmdIDToQCY` (`earphonesetup/manager/ZRDeviceImpl.java:29-46, 80-94`).
The result is cached in `ZRDeviceImpl.kfHashMap`. Later writes need this cache.

| Gesture        | QCY id (app, panels) | **ZR id (on the wire)** |
|----------------|----------------------|-------------------------|
| Left single    | 1 | **1** |
| Right single   | 2 | **4** |
| Left double    | 3 | **2** |
| Right double   | 4 | **5** |
| Left triple    | 5 | **3** |
| Right triple   | 6 | **6** |
| anything else  | n | n (identity) |

So ZR numbers keys left 1–3 (single, double, triple) and right 4–6, while QCY interleaves L/R.

Example read (hypothetical values): `01 00 02 01 03 04 04 00 05 01 06 07` →
ZR {1:0, 2:1, 3:4, 4:0, 5:1, 6:7} → QCY {L-single:0, L-double:1, L-triple:4, R-single:0,
R-double:1, R-triple:7}.

Function values pass through unchanged (§3.2 table).

> jadx bug: the normal jadx output of `converCmdIDToZR` (`ZRDeviceImpl.java:48-67`) seems to map
> 6 → 5. The bytecode-level output (`research/decompiled/fallback_zr/…/ZRDeviceImpl.java:99-122`,
> produced with `jadx --decompilation-mode fallback`) shows `6 → 6`. The table above is
> correct. `converCmdIDToZR` is the exact inverse of `converCmdIDToQCY`.

### 2.5 EQ: `0x000B` (ZR blob)

`QCYConnectManager.readEQData`, ZR branch (`QCYConnectManager.java:830-842`):

```java
readBleData(mac, UUID_EQ, (…, bArr) ->
    onEQDataChange(mac, new EQDataBean(new byte[]{bArr[120], bArr[121], bArr[122],
                                                  bArr[123], bArr[124], bArr[125]})));
```

So the read value is the same 144-byte blob that is written (§3.1):

* `bArr[120]` = active EQ id (`eqType`): 1–6 preset, 255 custom, 128 custom reset;
* `bArr[121..125]` = the first five gains, signed dB.

The app ignores the rest (coefficients, extra gains, frequencies, checksum). The read needs at
least 126 bytes, so it is longer than a default-MTU read. Android/BlueZ handle long reads
(Read Blob) automatically.

**[unknown]** Whether `0x000B` notifies on ZR. If it does, the generic notification handler
(`QCYHeadsetClient.java:818-828`) would parse it as a *legacy* EQ (`byte 0` = type), which is
wrong for a ZR blob. That suggests it does not notify, or the app misparses it. A Rust client
should parse ZR EQ notifications with the ZR layout, and may verify the checksum.

---

## 3. Writes

### 3.1 EQ: `0x000B`, 144-byte biquad blob (`ZRDeviceImpl.setEQ`)

Call sites: `QCYConnectManager.setEQData` (`QCYConnectManager.java:637-640`) and `setEQMarketData`
(`:800-804`). Both write `zrDevice.setEQ(eqType, gains, null, null)` to `UUID_EQ` with
`sendBleData`. Because Q and frequencies are passed as `null`, **the defaults are always used**.
ZR is explicitly excluded from the parametric multi-EQ (`0x20`/`0x22`) path
(`QCYConnectManager.java:470, 738`). That matters because the 5-band panels contain `q`/`freq`
keys that would otherwise switch it on.

Encoder: `earphonesetup/manager/ZRDeviceImpl.java:120-242`.

#### Inputs

* `eqType` (`int`, written as one byte): sys-EQ id from the panel. `sys_eq` entries have no
  `"id"`, so `ControlpanelJSONManager.analyseEQJSON` assigns `id = position + 1`
  (`manager/controlpan/ControlpanelJSONManager.java:735-746`). For ZR panels that gives
  **1 Default, 2 Pop, 3 Bass, 4 Rock, 5 Soft, 6 Classic**. A custom curve is sent as **255**
  (`setEQData` maps 240 → 255, `QCYConnectManager.java:665`; the editor uses 255,
  `v2ui/view/eq/CustomEQLayout.java:389`). "Reset custom" is **128** (`CustomEQLayout.java:454`).
* `gains` (`byte[]`): signed **integer dB**, one per panel band. Presets use
  `SysEQSeted.getGainRealArray()` = panel `eqs` value ×10 / 10 (`model/controlpanel/SysEQSeted.java:316-331`,
  `view/itempage/EQItemPagerView.java:350-352`). Panel range ±8 dB (`mindb`/`maxdb` 8). Length is
  **5** for family-A panels and **10** for family-B/C panels. Only the first 5 drive filters.
* Default Q per band: `[1.5, 1.5, 3, 3, 3]`. Default centre frequency (Hz): `[62, 250, 1000, 4000, 8000]`
  (`ZRDeviceImpl.java:197-198`; `Logger.MAX_LOG_BYTES` = 4000, `com/king/logx/logger/Logger.java:22`).
  These match the family-A panel `"freq": "62,250,1K,4K,8K"`, `"q": "1.5,1.5,3,3,3"`.

#### Layout (always 144 bytes)

| Offset | Size | Content |
|--------|------|---------|
| 0      | 120  | 5 biquad bands × 6 coefficients × int32 **little endian**, Q2.21 fixed point (1.0 = `0x00200000`) |
| 120    | 1    | `eqType` |
| 121    | N    | the N gain bytes as given (signed dB). N = 5 or 10 |
| 121+N  | 2    | `5A 5A` |
| 123+N  | 10   | 5 × u16 LE frequency (`62, 250, 1000, 4000, 8000`) → `3E 00 FA 00 E8 03 A0 0F 40 1F`. **Omitted (left zero) when `eqType == 1`** |
| …      | …    | zero padding |
| 143    | 1    | checksum |

With N = 10 the layout fills bytes 0–143 exactly (120 + 1 + 10 + 2 + 10 + 1).

**Band loop.** For `i` in `0..gains.len()`: freq = `freqs[i]` if `i < 5` else `i*1000`, Q =
`qs[i]` if `i < 5` else 3.0. Stop when the next 24-byte band would reach byte 144
(`(25 + len) − 1 >= 144`, line 209). So **at most 5 bands are encoded**, and gains 6–10 only
appear in the gain list.

**`eqType == 1` (Default) special case** (lines 180-195): the coefficients are five *flat*
bands `[2097152, 0, 0, 2097152, 0, 0]` whatever the gains are, and the frequency list is not
written. The gains are still copied to 121….

**Coefficients per band** (`getCoeff`, lines 120-146). This is the RBJ peaking filter at
fs = 44100 Hz. If gain == 0 the band is flat `[1,0,0,1,0,0]`. Otherwise:

```
A     = 10^(gain_dB / 40)
w0    = 2π·f / 44100
Qe    = (10·Q / 7) · √2 / 2            // ≈ 1.0102·Q
alpha = sin(w0) / (2·Qe)
a0    = 1 + alpha / A
c[0]  =  Q21((1 + alpha·A) / a0)        // b0
c[1]  =  Q21(-2·cos(w0) / a0)           // b1
c[2]  =  Q21((1 - alpha·A) / a0)        // b2
c[3]  =  Q21(1.0)  = 2097152            // a0 normalised
c[4]  = -Q21(-2·cos(w0) / a0)           // -a1   (negated after rounding)
c[5]  = -Q21((1 - alpha/A) / a0)        // -a2   (negated after rounding)
Q21(x) = (int)(long)(x · 2^21 + 0.5)     // Java cast: truncates toward zero
```

To match the app bit for bit, keep the "+0.5 then truncate toward zero" rounding (it is *not*
round-half-away for negative values) and negate **after** rounding for c[4] and c[5].

**Checksum** (lines 152-173, 218-239, confirmed in bytecode):
`byte[143] = ( Σ coefficient bytes + 0x5A + 0x5A + Σ gain bytes + Σ (freq_lo + freq_hi) ) mod 256`.
That equals **the sum of bytes 0–142 except the `eqType` byte at 120, mod 256**. The `eqType`
byte is *not* summed.

No sequence number and no header. One ATT write of 144 bytes.

#### Byte-level examples

Computed with a port of the Java code that was checked against a literal transliteration of
`setEQ`/`getByteFromArray` (all cases matched).

**Preset 1 "Default", 5 gains `0,0,0,0,0`** (eqType 1 → flat, no frequency list):

```
000: 00 00 20 00 00 00 00 00 00 00 00 00 00 00 20 00 00 00 00 00 00 00 00 00   band 1
024: 00 00 20 00 …  (bands 2–5 identical)
120: 01 00 00 00 00 00 5a 5a 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 f4
```
Checksum: 5 × (0x20 + 0x20) + 0x5A + 0x5A = 0x1F4 → `F4`.

**Preset 2 "Pop" on a 5-band panel (T1C/M10/T2C), gains `5,-2,-4,-3,6`:**

```
bands: [2100712,-4184992,2084444,2097152,4184992,-2088004]   62 Hz +5
       [2091539,-4137095,2048183,2097152,4137095,-2042570]  250 Hz -2
       [2074979,-4032859,1999162,2097152,4032859,-1976990]   1 kHz -4
       [2038550,-3193525,1754447,2097152,3193525,-1695845]   4 kHz -3
       [2297404,-1584139,1494491,2097152,1584139,-1694743]   8 kHz +6
000: e8 0d 20 00 60 24 c0 ff 5c ce 1f 00 00 00 20 00 a0 db 3f 00 bc 23 e0 ff
024: 13 ea 1f 00 79 df c0 ff b7 40 1f 00 00 00 20 00 87 20 3f 00 36 d5 e0 ff
048: 63 a9 1f 00 a5 76 c2 ff 3a 81 1e 00 00 00 20 00 5b 89 3d 00 62 d5 e1 ff
072: 16 1b 1f 00 4b 45 cf ff 4f c5 1a 00 00 00 20 00 b5 ba 30 00 9b 1f e6 ff
096: 3c 0e 23 00 f5 d3 e7 ff db cd 16 00 00 00 20 00 0b 2c 18 00 e9 23 e6 ff
120: 02 05 fe fc fd 06 5a 5a 3e 00 fa 00 e8 03 a0 0f 40 1f 00 00 00 00 00 04
```

**Custom (255) on a 10-band panel, gains `3,0,-2,0,4,0,0,0,0,0`:**

```
000: 45 08 20 00 ba 28 c0 ff a6 cf 1f 00 00 00 20 00 46 d7 3f 00 16 28 e0 ff   62 Hz +3
024: 00 00 20 00 00 00 00 00 00 00 00 00 00 00 20 00 00 00 00 00 00 00 00 00   250 Hz flat
048: d8 d4 1f 00 69 45 c2 ff 82 87 1e 00 00 00 20 00 97 ba 3d 00 a6 a3 e1 ff   1 kHz -2
072: 00 00 20 00 00 00 00 00 00 00 00 00 00 00 20 00 00 00 00 00 00 00 00 00   4 kHz flat
096: d3 fd 21 00 90 1b e8 ff df 32 17 00 00 00 20 00 70 e4 17 00 4e cf e6 ff   8 kHz +4
120: ff 03 00 fe 00 04 00 00 00 00 00 5a 5a 3e 00 fa 00 e8 03 a0 0f 40 1f 15
```

(Python reference: `research/notes/` has no script. The cross-checked port is in the session
scratchpad; it is ~60 lines and can be rewritten from the formulas above.)

#### Unused tables

`ZRDeviceImpl.eq1`…`eq5` (`ZRDeviceImpl.java:18-22`, 36 ints each) are never referenced. They
look like 5 bands × 6 coefficients plus 6 packed tail ints (for example `-67041789, 1025`), which
may be an older blob layout **[unknown]**. Do not use them.

### 3.2 Key functions: `0x000D`, full table in ZR order

`QCYConnectManager.setKeyFunction`, ZR branch (`QCYConnectManager.java:1527-1532`):

```java
zrDevice.setKeyFunctionItem(eventId /*QCY id*/, fn);           // kfHashMap[eventId] = fn
sendBleData(mac, UUID_KEY_FUNCTION_V2, zrDevice.getKeyFunctionBytes());
readKeyFunction(mac, uuid);                                     // read back
```

`getKeyFunctionBytes` (`ZRDeviceImpl.java:96-110`) writes **every cached entry**, not just the
changed one, as `[converCmdIDToZR(qcyId), fn]` pairs in `HashMap<String,…>` iteration order. For
keys `"0"`…`"6"` that is ascending QCY id, so the ZR ids come out as 1, 4, 2, 5, 3, 6. The panel
UUID (`0x0001`…`0x0006` for V1 panels) is ignored for ZR.

Requirements and edge cases:

* The cache only exists after a successful `0x000D` read (§2.4). If there was no read,
  `setKeyFunctionItem` does nothing and `getKeyFunctionBytes` returns `null`. The app then writes
  an empty value. The Rust client should read before writing.
* Padding pairs `00 00` from the read collapse into one key `"0"` and are written back as `00 00` first.

`eventId` comes from the panel. V1 panels (`type 1`) derive it from the characteristic
(`EarphoneKey.getEventIDFromUUIDV1`, `model/controlpanel/EarphoneKey.java:28-58`:
`0x0001`→1, `0x0002`→2, …, `0x0006`→6). V2 panels (`type 10`, QT12S) give `eventID` 3–6 directly.

**Function codes** (byte 2 of each pair; raw panel `cmd`, no conversion):

| fn | Meaning (ZR panels) |
|----|---------------------|
| 0  | Not work / none (T13 single touch only) |
| 1  | Play/pause |
| 2  | Previous track |
| 3  | Next track |
| 4  | Voice assistant |
| 5  | Volume up |
| 6  | Volume down |
| 7  | Gaming mode / "Low latency mode" (T7, T8, T13, QT12S only) |

"Answer/end call" is shown as a fixed call-mode gesture (double touch). It cannot be set.

Example: right triple (QCY 6) → volume up (5), after a read of the table in §2.4:

```
write 0x000D: 01 00 04 00 02 01 05 01 03 04 06 05
              L1:0  R1:0  L2:1  R2:1  L3:4  R3:5
```

**[unknown]** Whether the firmware accepts a single changed pair (the generic QCY path sends only
the changed pair, `docs/PROTOCOL.md` §8). Mirror the app and send the full table.

### 3.3 Time sync: `0x000E` (+ fallbacks)

`QCYConnectManager.sendTime` (`QCYConnectManager.java:2203-2219`), with `t = Unix seconds (UTC)`:

1. ZR only: write `0x000E` ← `00 00 t3 t2 t1 t0` (u32 **big endian**).
2. All families: write `0x000C` ← `t3 t2 t1 t0`. **If `0x000C` does not exist**, write `0x1001` ←
   TLV `FF 06 0F 04 t3 t2 t1 t0` instead.

Example for 2026-10-13 00:00:00 UTC (t = `0x6ACD7480`): `0x000E` ← `00 00 6a cd 74 80`.

When the app does this: after each BLE connect it sets `isSendTime = true`
(`v2ui/fragment/DeviceInfoFragment.java:730`, `v2ui/activity/V2ConnectEarphoneActivity.java:219`).
**Before the first custom-EQ write** of the session it calls `sendTime`, sleeps 300 ms, then
sends the EQ (`v2ui/view/eq/CustomEQLayout.java:691-697`). Byte 0 = `00` is probably a sub-command
selector for `0x000E` (`00` = time, `01` = reset, `02` = clear pairing; §3.4). Whether ZR firmware
*needs* the time before it accepts EQ is **[unknown]**. Doing the same is cheap and safe.

### 3.4 Reset / clear pairing (panel-driven)

`SingleButtonItemView` writes the panel `cmd` hex string as raw bytes to the panel `character`
(default `0x1001`) after a confirmation dialog (`v2ui/view/SingleButtonItemView.java:161-163, 207-210`).
Afterwards it re-reads language, `0x14`, settings, EQ and keys.

| Panel family | Reset to default | Delete pairing records | Factory reset |
|--------------|------------------|------------------------|---------------|
| A: T1C, M10, T2C | `0x000E` ← `01` | `0x000E` ← `02` | — |
| B: T9, T7, T8, T1S, T13 | `0x000A` ← `01` | `0x000A` ← `02` | — |
| C: QT12S | `0x1001` ← `FF 02 01 00` | — | `0x1001` ← `FF 02 03 00` |

These are destructive. Expose them only behind a confirmation, as the app does.

### 3.5 In-ear sensor (T7, T8): `0x000F`

Panel item `type 201` "In-ear sensor", `character 0x000F`, `sub_items` `01` = on, `02` = off
(`19858.json`, `19859.json`). `SingleSwitchItemView.switchChange` writes the matching `cmd` to the
`character` with `sendBleData` (`v2ui/view/SingleSwitchItemView.java` ~137-176, write at line 172).

* On: `0x000F` ← `01`
* Off: `0x000F` ← `02`

The read-back for ZR comes from `0x000E` byte 1 (§2.3), not `0x000F`. **[unknown]** whether
`0x000F` is also readable on ZR. Non-ZR devices return `[inEar, cmd10]` there.

### 3.6 Sleep mode (T13 only): standard TLV on `0x1001`

Panel item `type 204` "Sleeping Mode", `cmdid 16`: `FF 03 10 01 01` (on) / `FF 03 10 01 02` (off),
identical to `docs/PROTOCOL.md` id `0x10`. Read-back would arrive as a `0x10` TLV on `0x1002`
if the device has it **[unknown]**.

### 3.7 Not available on ZR

None of the nine ZR panels has ANC/transparency (`type 6`/`9`), a game-mode switch, volume,
balance, prompt tone or language, wear detection (`0x2C`), auto power-off, LDAC or dual device.
The code has no ZR branch for any of them. `setNoiseMode`, `diyanshi` and so on would go to
`0x1001` and be dropped if it is missing. Game mode exists only as key action 7.

---

## 4. How the app's UI differs for ZR

* **Routing:** `isZRDevice()` branches only in `QCYConnectManager` (EQ write/read, key write/read
  conversion, settings read, time sync, instance bookkeeping at `:84, :93, :325-352, :1722-1734`)
  and in `CustomEQLayout`. A grep of the whole `com/qcymall` tree finds no other ZR checks.
* **Connect** (`QCYConnectManager.java:1722-1734`): it creates a fresh `ZRDeviceImpl(mac)`, which
  clears the key-table cache. It is stored both per MAC and in a single `zrDevice` field (the last
  connected one wins, which is an app bug with two ZR devices). The usual
  `requestCMDData(68)` (= `0x1001` ← `FF 03 FE 01 44`, MAXEQ_COUNT) is still sent 1 s later
  (`:1746`) and is dropped if `0x1001` is absent.
* **Initial sync** (`DeviceInfoFragment.java:720-735, 790-830`): battery (`0x0008`) → EQ
  (`0x000B`, §2.5) → keys (`0x000D`, §2.4) → settings (`0x000E`, §2.3) → version (`0x0007`), about
  60 ms apart, plus language/`readCmdIndexList` reads that are empty for ZR panels.
* **EQ screen:**
  * Presets come from panel `sys_eq` (ids 1–6). Family A has 5 gains (62/250/1k/4k/8k);
    families B/C have 10 gains labelled 31…16k, **but only the first 5 gains reach the
    filters**, at 62/250/1k/4k/8k. So the 10-band presets' sound differs from their labels.
    Per-preset `freq`/`q` keys in family A (for example Bass `freq 31,62,125,500,1000`) are
    **ignored** by the ZR encoder. The extra `vals` key is never read by the app.
  * The custom editor labels the x-axis `31, 125, 500, 2k, 8k` for ZR
    (`SysEQSeted.getFreqStringsZR`, `SysEQSeted.java:119-121`, used at `CustomEQLayout.java:112-113`),
    while the device filters are at 62/250/1k/4k/8k. The gain display uses the plain byte array,
    not the multi-EQ model (`CustomEQLayout.java:134-138, 493`).
  * Range ±8 dB, integer steps.
* **Key screen:** V1 per-gesture layout (`type 1`; families A/B) or V2 (`type 10`, QT12S). The ids
  are in QCY numbering in the UI and converted only at the wire boundary (§2.4/§3.2).
* **Battery:** display rounded to 10 % (§2.1).
* **Panels:**

  | Family | Models | Keys | EQ | Extra items |
  |--------|--------|------|----|-------------|
  | A | T1C 19860, M10 19861, T2C 19863 | V1: L/R single, double, triple; fn 1–6 | 5 bands, q 1.5,1.5,3,3,3 | reset/clear via `0x000E` |
  | B | T9 19857 (single+double only), T1S 19862 (single+double), T7 19858 & T8 19859 (double+triple, fn 1–7), T13 19866 (single+double+triple, fn 0–7) | V1 | 10 gains (5 used) | reset/clear via `0x000A`; T7/T8 in-ear `0x000F`; T13 sleep `0x1001` |
  | C | QT12S 19865 | V2: eventID 3–6 (double, triple), fn 1–7 | 10 gains (5 used) | reset/factory reset via `0x1001` TLV |

  Every panel also has "Find headset" (`type 3`, map/location in the app; no ZR-specific command).

---

## 5. Standard QCY pieces that still apply unchanged

| Feature | Characteristic / command | Notes |
|---------|--------------------------|-------|
| Service, discovery, notify-all, MTU | `0xA001` | identical |
| Advertisement parsing | company id `0x521C` | identical (`docs/PROTOCOL.md` §2) |
| Battery | `0x0008` read + notify | identical; app rounds to 10 % |
| Firmware version | `0x0007` read | identical |
| Key table container | `0x000D` `[id, fn]` pairs | same framing; **key ids remapped** (§2.4) and full table written |
| TLV framing | `0x1001`/`0x1002`, `FF len {id n payload}` | only where the panel uses it (T13 sleep `0x10`, QT12S reset `0x01`/`0x03`), plus the automatic `FE 01 44` request and the `0x0F` time fallback. Presence on ZR **[unknown]** |
| Legacy reset characteristic | `0x000A` ← `01` / `02` | family-B panels |
| Legacy time characteristic | `0x000C` ← u32 BE seconds | tried by `sendTime` |
| In-ear write | `0x000F` ← `01`/`02` | T7/T8 |

**Not** unchanged:

* `0x000B` carries the 144-byte ZR blob, not `[preset, g0…g9]`.
* `0x000E` is the ZR status/time/reset characteristic.
* Parametric EQ TLVs `0x20`/`0x22` are never used for ZR.

---

## 6. Open questions to settle on hardware (read-only checks first)

1. Full characteristic list and properties of `0xA001` on a ZR unit (presence of `0x1001`,
   `0x1002`, `0x000A`, `0x000C`, `0x000F`, `0x0001`–`0x0006`). Confirm no ZR-only service exists.
2. Raw `0x000E` read: length, meaning of bytes 0 and 2+, bit 7 of byte 1.
3. Raw `0x000B` read: is it the 144-byte blob as last written (checksum valid)? Does it notify?
4. Raw `0x000D` read: number of pairs and padding.
5. Whether the EQ write is accepted without the prior `0x000E` time write, and whether the
   device validates the checksum.
6. Whether a single-pair `0x000D` write is accepted.
