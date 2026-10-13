# JieLi (杰理) earbuds in the QCY app: implementation spec

Source: QCY Android app 4.0.7, decompiled with jadx (`research/decompiled/sources/`).
All paths below are relative to that directory unless stated otherwise.
Nothing here was verified against a real JieLi device. Statements marked
**(uncertain)** are inferences, not read directly from code.

> Project rule: no firmware update. Everything about OTA/DFU is skipped except
> the opcode blacklist in §8.

## 0. Summary

* QCY does **not** use the big JieLi SDK (`com.jieli.jl_rcsp`, which has the
  `libjl_auth.so` authentication) for earbuds. It uses the small
  **`com.jieli.jl_bt_rcsp`** library (123 classes, no native code, **no
  authentication**). QCY's wrapper is
  `com/qcymall/earphonesetup/manager/JLDeviceImpl.java` (`extends BtRcspOpImpl`).
* Transport is **BLE only**, on a QCY-specific GATT service **`0xA002`**
  (write `0x0001`, notify `0x0002`), *not* JieLi's usual `AE00/AE01/AE02`.
* Packets are standard JieLi **RCSP** frames: `FE DC BA | flags | opcode |
  len(BE16) | [status] sn | params | EF`, no CRC.
* Almost every QCY feature is sent as QCY's own `id len payload` TLV wrapped
  in the RCSP **custom command, opcode `0xFF`** (same ids as `docs/PROTOCOL.md`).
  "Old" JL models additionally use RCSP *ADV attributes* (opcodes `0xC0/0xC1`)
  for keys, in-ear detection, sleep mode, game mode, battery, name, balance and
  a JieLi-style noise mode; EQ presets go through RCSP sys-info (`0x07/0x08`).
* The `libjl_auth.so` challenge/response (only relevant if a device ever asks
  for it) is the **Bluetooth E1 function (SAFER+)** with a fixed key and
  address; it can be implemented in Rust without the native library (§3).

## 1. Transport and discovery

### 1.1 Which devices are JieLi

`com/qcymall/earphonesetup/model/Devicebind.java`:

| Line | Method | Condition on `vendorId` (= model id from the advert) |
|------|--------|------------------------------------------------------|
| 924  | `isJLDevice()` | `(id & 0xF0) == 0x70 \|\| id == 512` |
| 934  | `isNewJLDevice()` | `(id & 0x80F0) == 0x8070` (e.g. `0x8071…0x807E` = 32881–32894, `0x9071` = 36977) |
| 191  | `isOTAOnlyDevice()` | `(id & 0xF00) == 0x200 \|\| id == 16823 \|\| id == 16825`; id 512 is such a device. **Do not control these** (they exist only for firmware update). |
| 929  | `isOnly10Freq()` | true for all JL (`(id & 0xF0) == 0x70`) |
| 196  | `isH2()` | 16753, 16754, 16756 (app re-reads EQ on every battery poll) |
| 938  | `checkCanNotify()` | false for a fixed list of older ids (6513–6517, 16753–16758, 16760, 16762, 16767, 18289–18295, 18545, 18801–18815, 19057–19070, 19313, 19569–19576, 19825, 19826, 22897, 23153–23159). For those the app polls ANC (`0x17`) every 3 s instead of relying on notifications. |

### 1.2 Advertisement

JL earbuds advertise exactly like other QCY earbuds: manufacturer data with
company id `0x521C` (or `0x522C`), parsed by `Devicebind.refreshInfoFromAVData`
(`Devicebind.java:558`): model id BE at 0–1, colour, battery L/R/case at 5/6/7
(bit 7 = charging), classic address of this bud at 11–16 and of the peer at
18–23. There is **no JL-specific parsing** of this record; see
`docs/PROTOCOL.md` §2.

`BleScanManager.java:153-170` also looks at JieLi's company id
`BluetoothConstant.JL_VID = 1494 = 0x05D6`
(`com/jieli/bluetooth_connect/constant/BluetoothConstant.java:26`). Only one
shape is handled: data starting with ASCII `"ATOLJ"` (bytes `41 54 4F 4C 4A`,
i.e. "JLOTA" reversed), followed at bytes 6–11 by a reversed address
(`%02X:` of bytes 11,10,9,8,7,6). This is a device **in firmware-update (DFU)
mode** (`EVENT_ACTION_EARPHONEFOUND_DFUADDR`). A desktop client should detect
it and leave the device alone. Other `0x05D6` adverts are ignored.

The scan filter list (`BleScanManager.java:300-309`) is: company `0x521C`
(any data), company `0x522C` with data mask, company `0x05D6` (any data).

### 1.3 GATT

The BLE link is opened by the generic `QCYHeadsetClient.connectDevice`
(`com/qcymall/qcylibrary/QCYHeadsetClient.java:229-300`), the same code as for
WuQi/Realtek models:

1. Connect, discover services (retry 3, timeout 20 s).
2. Look up service **`0000A001-…`**; enable notifications on every notifying
   characteristic of A001. `onConnectionStateChange(…, 2)` is only raised if
   **A001 exists**, so JL QCY earbuds must expose A001 as well
   **(uncertain which characteristics it has on JL firmware; the app never
   reads or writes A001 for JL devices except the prompt-language write in
   §5.12)**.
3. Request MTU 512 (`inuker …/BleChangeMTURequest.java:29`); stored as
   `mtu − 3`, minimum 20.
4. `QCYConnectManager` (`manager/QCYConnectManager.java:1705-1721`) sees
   `isJLDevice()` and creates `JLDeviceImpl(context, bleDevice, isNewJLDevice)`,
   then calls `setConnectedStauts(4)`.

`JLDeviceImpl` (`JLDeviceImpl.java:93-95, 263-285, 335-359`):

| Role   | UUID |
|--------|------|
| Service | `0000a002-0000-1000-8000-00805f9b34fb` |
| Write (phone → buds) | `00000001-0000-1000-8000-00805f9b34fb` |
| Notify (buds → phone) | `00000002-0000-1000-8000-00805f9b34fb` |

(The UUID constants are borrowed from `ControlerPanl.UUID_KEY_FUNCTION_CL1_V1`
/ `CR1_V1`; they are looked up under service A002. The same A002 service is used
by QCY's JL OTA code, `ota/jldevice/OTAManager.java:31-33`, so the update
protocol runs over this channel too: see §8.)

* Notifications: `unnotify` then `notify` on A002/0002; every notification is
  fed raw into the RCSP stream parser (`callbackReceiveDeviceData`).
* Writes: one queued GATT write per RCSP frame, the next write starts in the
  completion callback of the previous one (`sendDataToDevice`/`writeData`,
  lines 335-359). The inuker library uses Android's default write type for the
  characteristic (write-without-response if the characteristic advertises it,
  else write request).
* **No fragmentation on send**: a frame is written in one ATT write, so it must
  fit `MTU − 3`. With the MTU of 512 that Android negotiates this is always
  true. On desktop: negotiate a large MTU (BlueZ/WinRT do this automatically);
  the largest frame QCY sends is a parametric EQ (≈ 85 bytes). If the MTU
  were 23, splitting the frame across writes would probably work because the
  firmware uses the same stream parser **(uncertain)**.
* **Reassembly on receive**: required, a response may be split over several
  notifications (§2.3).
* Classic SPP is **not** used for JL control (`BluetoothSPPManager` is WuQi
  only; `JLOTASPPManager` is OTA).
* No pairing/bonding is needed on BLE (same as other QCY models).

Only one JL connection per device is kept (`QCYConnectManager.mJLDevice`,
keyed by BLE address).

## 2. RCSP framing

Code: `com/jieli/jl_bt_rcsp/tool/ParseHelper.java` (`packSendBasePacket`
322-389, `findPacketData` 63-161, `parsePacketData` 173-227),
`…/data/model/base/BasePacket.java`, `…/tool/data/NewDataHandler.java`.

### 2.1 Frame layout

```
+----------+-------+--------+-----------+-------------------------------+------+
| FE DC BA | flags | opcode | len (BE16)| body (len bytes)              |  EF  |
+----------+-------+--------+-----------+-------------------------------+------+
flags: bit7 = 1 command / 0 response
       bit6 = 1 "needs response" (only set on commands)
       bits 0-5 = 0
body, command  (bit7=1): sn,          [xmOpCode if opcode==0x01], params…
body, response (bit7=0): status, sn,  [xmOpCode if opcode==0x01], params…
```

* `len` counts the body only (from `sn`/`status` to the last param byte); it
  excludes the 3 prefix bytes, flags, opcode, the length itself and `EF`.
  Total frame size = `len + 8`.
* No CRC/checksum.
* Max `len` accepted on receive: `MAX_RECEIVE_MTU = 530` (larger frames are
  discarded). Max on send: `MAX_COMMUNICATION_MTU = 530`, updated from target
  info attribute 13 or a `0xD1` response (never sent by QCY).
* `xmOpCode` exists only for opcode `0x01` (DATA), which QCY doesn't use.
* All multi-byte integers in RCSP itself are **big endian**
  (`CHexConverter.intToBigBytes`, `int2byte2`, `bytesToInt`). QCY's TLV
  payloads inside the custom command keep their own little-endian fields.

### 2.2 Sequence numbers, responses, timeouts

* `sn` is one byte from a global counter, starting at 0, post-incremented and
  wrapping 255 → 0 (`util/BluetoothUtil.java:25-36`, used in
  `BtRcspOpImpl.sendCommandAsync`, line 123).
* A response is matched to its command by **(opcode, sn)**
  (`NewDataHandler.findSendDataInfo`).
* Every command QCY sends is a "param + response" command (type 2), i.e.
  flags `0xC0`.
* Timeout 2000 ms (500 ms minimum). On timeout the *same* frame (same sn) is
  re-sent up to 3 times, then the command fails (`NewDataHandler.CmdTimeOut`).
* Response `status`: 0 success, 1 fail, 2 unknown command, 3 busy, 4 no
  resource, 5 CRC error, 6 all-data CRC error, 7 parameter error, 8 response
  data over limit (`constant/StateCode.java`). Target info is retried 3× every
  500 ms on status 3 (`BtRcspOpImpl.java:560-571`).
* Frames from the device with bit7 = 1 are **device-originated commands**
  (notifications). The app **never answers them**, even when bit6 is set
  (`sendCommandResponse` exists but has no caller).

### 2.3 Receive parser

`findPacketData` keeps a static cache between notifications:

1. Prepend cached bytes.
2. Scan for `FE DC BA`. If the buffer ends inside the header or before
   `len` is complete, cache from the header and wait.
3. If `len > 530`, skip.
4. If the byte at `header + 7 + len` is `EF`, emit the packet and continue
   after it; otherwise advance one byte and rescan.
5. If the frame is incomplete, cache from the header.

A Rust implementation: a `Vec<u8>` stream buffer, resync on `FE DC BA`,
require the `EF` trailer, drop garbage before the header.

### 2.4 LTV sub-format used by several opcodes

Target info, ADV info, sys-info attributes and the ADV *set* payload all use

```
len  type  data[len-1]        (len counts type + data)
```

(`ParseDataUtil.packLTVPacket`, `convertParamDataToAttrBeans`,
`ParseHelper.parseTargetInfo/parseADVInfo`).

### 2.5 Example frames (constructed from the code, not captured)

| Purpose | Frame |
|---------|-------|
| Get target info, all attributes, sn 0 | `FE DC BA C0 03 00 06 00 FF FF FF FF 00 EF` |
| Set ADV attr 7 (time) = 0x6529A1B0, sn 1 | `FE DC BA C0 C0 00 07 01 05 07 65 29 A1 B0 EF` |
| Custom: read QCY id 0x2F (battery), sn 2 | `FE DC BA C0 FF 00 04 02 FE 01 2F EF` |
| …its response (L 90 %, R 92 %, case 100 %) | `FE DC BA 00 FF 00 07 00 02 2F 03 5A 5C 64 EF` |
| Custom: ANC mode 1 scene 1 level 2, sn 3 | `FE DC BA C0 FF 00 06 03 17 03 01 01 02 EF` |
| Get ADV info, all (mask 0xFFFFFFFF), sn 4 | `FE DC BA C0 C1 00 05 04 FF FF FF FF EF` |
| Get EQ (sys info fn 0xFF, mask 0x10), sn 5 | `FE DC BA C0 07 00 06 05 FF 00 00 00 10 EF` |

## 3. Authentication

### 3.1 What QCY does for earbuds: nothing

* `com.jieli.jl_bt_rcsp` contains no `native` method, no `loadLibrary`, no
  auth class (`grep -rn "native\|loadLibrary" com/jieli/jl_bt_rcsp` is empty).
* Its connect handler `BtRcspOpImpl.handleDeviceConnectedEvent`
  (`impl/BtRcspOpImpl.java:413-425`) immediately sends GET_TARGET_INFO and
  "set connected time". No handshake precedes it.
* Even QCY's JL earbud OTA over A002 is configured with
  `setUseAuthDevice(false)` (`ota/jldevice/OTAManager.java:57`,
  `ota/jldevice/JLOTASPPManager.java:44`). Only the generic "JL SDK common"
  update path (`ota/jlcommon/JLSDKOTAManager.java:99`, used for DFU-mode /
  standard-JieLi devices) enables it.

**Conclusion: a desktop client should not authenticate; just start with
§4.** If a firmware unexpectedly stays silent, §3.2 is the fallback.

### 3.2 The JieLi RCSP auth (for reference / fallback)

Java side: `com/jieli/jl_rcsp/impl/RcspAuth.java` (and an equivalent in
`com/jieli/jl_bt_ota/impl/RcspAuth.java` using `libjl_ota_auth.so`). Natives
registered by `libjl_auth.so` for class `com/jieli/jl_rcsp/impl/RcspAuth`:
`nativeInit()Z`, `getRandomAuthData()[B`, `getEncryptedAuthData([B)[B`,
`setLinkKey([B)I`.

Auth bytes are written **raw** to the write characteristic (not RCSP framed);
replies arrive raw on the notify characteristic. Flow (`startAuth`,
`handleAuthData`):

1. Phone sends the "reset auth flag" RCSP frame
   `FE DC BA C0 06 00 02 00 01 EF` (opcode 0x06, sn 0, param 0x01), waits 500 ms.
2. Phone → device: `00 ‖ R1` (17 bytes, R1 = 16 random bytes).
3. Device → phone: `01 ‖ E1(K, R1, A)` (17 bytes). Phone checks it.
4. Phone → device: `02 70 61 73 73` ("\x02pass").
5. Device → phone: `00 ‖ R2`; phone → device: `01 ‖ E1(K, R2, A)`.
6. Device → phone: `02 70 61 73 73`: authenticated.
   Timeouts 3 s, 2 retries of step 2.

Valid auth packets: 17 bytes starting with 0/1, or 5 bytes starting with 2.

Native algorithm, from disassembly of `libjl_auth.so` (arm64; saved as
`research/decompiled/native/libjl_auth.s` and `libjl_auth.data.txt`, made with
`llvm-objdump-21`):

* `getRandomAuthData` (0x1d98): `[0x00, rand()×16]`.
* `getEncryptedAuthData` (0x1f44): output `[0x01, 16 bytes]`, the 16 bytes
  are `function_E1test(addr=@0xa3f0, rand=input[1..17], key=@0xa3e0, out)`.
* `function_E1test` (0x10c8) is exactly **Bluetooth E1** (Core spec, Vol 2
  Part H §6.3, SAFER+): key schedule(K); `Ar(K, RAND)`; XOR with RAND; byte-wise
  add of the address repeated cyclically (`addr[i mod 6]`); offset key K̃
  (`K[0]+233, K[1]^229, K[2]+223, K[3]^193, K[4]+179, K[5]^167, K[6]+149,
  K[7]^131, K[8]^233, K[9]+229, K[10]^223, K[11]+193, K[12]^179, K[13]+167,
  K[14]^149, K[15]+131`, verified instruction by instruction at
  0x1324-0x13e4); `Ar'(K̃, …)`. The full 128-bit output (SRES ‖ ACO) is
  returned. `.rodata` at 0x9e0 holds the SAFER+ exp table (`01 2D E2 93 BE 45
  15 AE …` = 45^i mod 257).
* Constants in `.data`:
  * link key K @0xa3e0: `06 77 5F 87 91 8D D4 23 00 5D F1 D8 CF 0C 14 2B`
    (replaceable through `setLinkKey`, which QCY never calls).
  * address A @0xa3f0: `11 22 33 33 22 11` (a palindrome, so byte-order
    questions don't arise).

So the auth can be implemented in pure Rust (a SAFER+ Ar/Ar' implementation,
≈150 lines). Check the implementation against the E1 sample data in the
Bluetooth Core spec, using the array index convention above (input byte
`i` = byte `i+1` of the 17-byte packet) **(byte order of the spec's test
vectors vs. this array convention is untested)**.

## 4. Connection sequence

What the app does after the GATT link and A002 notifications are up:

1. **GET_TARGET_INFO** `0x03`, params `mask(BE32)=0xFFFFFFFF, platform=0x00`
   (`BtRcspOpImpl.java:420`, `GetTargetInfoParam.getParamData`). Only sent if
   no target info is cached. On status 3 (busy) retried up to 3× after 500 ms.
2. **SET_ADV_INFO** `0xC0` attribute 7 ("connected time") =
   `u32 BE unix seconds` (`updateDeviceTime`, `BtRcspOpImpl.java:398-410`).
   Also sent again when the device sends `0xC4` with op 2.
3. ~1 s later the QCY layer asks for QCY id `0x44` (max EQ bands):
   custom `FE 01 44` (`QCYConnectManager.java:1746`). The JL path ignores the
   answer and always assumes 10 bands (`QCYConnectManager.java:529`).
4. The UI then reads what the model's control panel shows (§5): battery,
   version, keys, EQ, ANC, name, balance, and so on.

There is **no** "set communication way" (`0x0B`), no MTU command (`0xD1`), no
`0xC3` "enable ADV notify" and no keep-alive. Battery is **polled**: the device
page calls `readBattery` every 30 s (every 3 s for models where
`checkCanNotify()` is false, which also re-read ANC `0x17` each time),
`v2ui/fragment/DeviceInfoFragment.java:880-915`.

### 4.1 Target info response (`0x03`)

Params = LTV list (§2.4), `ParseHelper.parseTargetInfo` (lines 415-624):

| Type | Content |
|------|---------|
| 0 | protocol version, 1 byte, shown as `V_<hi nibble>.<lo nibble>` |
| 1 | `battery, volume, maxVol, flags` (flags bit0 = volume sync) |
| 2 | classic (EDR) address, 6 bytes in display order, `[profile, status]` |
| 3 | platform (1 byte) + license string |
| 4 | function mask (BE32) + current function (1 byte) |
| 5 | firmware version BE16: four nibbles, `V_a.b.c.d`. Old-JL "version" is taken from here (`JLDeviceImpl.getVersion`, lines 505-545, strips `V_`) |
| 6 | SDK type (2 or 4 → supports volume sync) |
| 7 | uboot version |
| 8, 9 | update-related flags (double backup / boot loader / **mandatory upgrade flag**). Ignore; QCY only logs `onMandatoryUpgrade`. Never act on it. |
| 10 | VID, PID, [UID], BE16 each |
| 11 | auth key (string) |
| 12 | project code (string) |
| 13 | protocol MTU: send BE16, [recv BE16] → max frame body length |
| 16 | name (deprecated) |
| 17 | BLE-only flag + BLE address (6) |
| 19 | bit0 MD5 supported, bit1 game mode active |

## 5. Features

Two command families:

* **QCY TLV in the custom command (`0xFF`)**: `JLDeviceImpl.sendCustomCMD`
  (lines 817-830) sends `CustomCmd(CustomParam(bytes))` = opcode `0xFF`,
  flags `0xC0`, params = the QCY TLV `id len payload…` (**no** `FF len` QCY
  frame header). Exactly one TLV per frame.
* **JieLi RCSP attributes**: ADV set/get `0xC0/0xC1` (§6) and sys info
  `0x07/0x08/0x09`.

### 5.1 Custom command details

* Write: `FF` params = `id n payload[n]`. Response status 0, params usually
  empty or an echo **(uncertain, the app ignores the write response)**.
* Read: `getCustomCMDInfo(id)` (lines 1011-1027) sends params
  `FE 01 id` (QCY "REQUESTDATA"). The response (`opcode 0xFF`, bit7 = 0) has
  params `id n payload[n]`, parsed by `checkCustomCMDInfo` (lines 837-1009).
* Unsolicited: the device may send a **custom command** (`0xFF`, bit7 = 1)
  whose params are a QCY TLV; `onDeviceCommand` (lines 151-163) passes it to
  the same `checkCustomCMDInfo`. This is how ANC/battery/etc. changes are
  pushed on models that can notify. Don't reply.
* `checkCustomCMDInfo` only accepts a buffer whose length is exactly
  `payload[1] + 2`.
* Generic setters from `QCYConnectManager` all map to it:
  `setSingleValue(id, v)` → `id 01 v` (line 1205); `setBytesValue`/`sendCMDData`
  → `id len bytes…` (lines 1219-1234, 1323-1334); `requestCMDData(id)` →
  `FE 01 id` (1275-1289). Panel switch items of type 200 send their `cmd` hex
  as is (e.g. dual device `24 01 01` / `24 01 00`, LDAC `23 01 01`, dynamic EQ
  `27 01 01`) (`v2ui/view/SingleSwitchItemView.java:160-170`).

Response parsing per id (`checkCustomCMDInfo`), where `p` is the payload:

| id | Meaning | Payload |
|----|---------|---------|
| 0x09 | gaming mode | `p[0] == 1` = on |
| 0x10 | sleep mode | `p[0] == 1` = on |
| 0x11 | fit test ("compactness") | `CompactnessDataBean(p)` |
| 0x14 | power-off timer | `PowerManagerDataBean(p)` (`[min LE16, left LE16]`) |
| 0x16 | balance | `p[0]` 0..100 |
| 0x17 | ANC | `[mode, scene, level]`; ignored if within 1 s of the app's own ANC write |
| 0x18 | pair name | UTF-8 |
| 0x1D | tone volume | `p[0]` |
| 0x1E | bool (take photo) | `p[0]` |
| 0x1F | standby | `p[0]` or LE16 if 2 bytes |
| 0x20 | parametric EQ, 6-byte bands | `EQParamListBean(p)` |
| 0x22 | parametric EQ, 7-byte bands | `EQParamListBean(p, true)` |
| 0x2B | key table | pairs `[gesture, action]` |
| 0x2F | battery | `[L, R, case]`, bit7 charging, bits 0-6 % |
| 0x30 | version | `[major, minor, patch]` (one version for both buds) |
| 0x4A | game config | raw |
| 0x4D | paired phones | `PairPhoneBeanList(p)` |
| 0x4F | phone status | `PhoneStatusBean(p)` |
| 0x51 | head-gesture config | `SensorFuncDataBean` (pairs) |
| 0x54 | AI ANC | `AIANCSettingDataBean` |
| 0x55, 0x56 | AI ANC config | `AIANCConfigDataBean` |
| 0x58 | hearing health | `p[0]<<24 \| p[1]<<16 \| p[3]<<8 \| p[2]` |
| other | | >2 bytes raw, 2 bytes LE16, 1 byte u8 |

The payload formats are the same as for WuQi models, see `docs/PROTOCOL.md`.

### 5.2 Battery

* New JL: custom read `0x2F` (`updateBattery`, line 1051).
* Old JL: GET_ADV_INFO mask `3361 = 0x0D21` (attrs 0 battery, 5 work mode,
  8 in-ear, 10 sleep, 11 noise mode), lines 1056-1085. Battery = ADV attr 0.
  The same response also refreshes in-ear, sleep, game mode (`workModel == 2`)
  and the noise-mode percentage.
* `updateSetting` (old, mask `-1`) also reports battery.

### 5.3 Version

New: custom read `0x30` → `maj.min.patch`. Old: target info attribute 5
(see §4.1), `V_` prefix removed.

### 5.4 Noise control (ANC / transparency)

Two paths, chosen by the panel, not by old/new:

1. **QCY scene ANC (most models, panel layout type 9 with packed cmd ids
   > 255)**: `QCYConnectManager.setNoiseMode` (lines 1462-1482) unpacks
   `mode = id>>16, scene = (id>>8)&0xFF, level = id&0xFF` and sends custom
   `17 03 mode scene level` (`JLDeviceImpl.setANCSenseMode`, line 610). Special
   case: `(3,1,0)` → `(3,2,0)`. Read with `FE 01 17`. Mode 1 ANC, 2 off,
   3 transparency, as in `docs/PROTOCOL.md` §6. Example panels: 16758 H3 Lite
   (`0x010000` ANC, `0x030000` transparency, `0x020000` normal); 32891 N65
   (`0x0101xx` noisy, `0x0102xx` commuting, `0x0103xx` indoor, `0x010400`
   wind, `0x010500` adaptive, transparency `0x0301xx`).
   The app throttles ANC writes to one per 500 ms (3 s for model 16758),
   `QCYConnectManager.checkCanANCSet`, lines 146-155.
2. **JieLi NoiseMode (panel layout type 11, "ancindex" + percent slider, e.g.
   DT10 models 18289/18290/18294/18295)**: `setNoiseLevelPercent(…, i4 ≤ 255)`
   (lines 1395-1451). Uses ADV attribute **11**:
   * get: GET_ADV_INFO mask `0x0800` → attr 11
     `mode, len, min(BE16), max(BE16), current(BE16)` (`NoiseMode.parseData`).
   * set: SET_ADV_INFO attr 11 with `NoiseMode.toData()` =
     `mode, len, min BE16, max BE16, current BE16` (always 8 bytes), where
     `current = min + (max − min) × percent / 100`. When only the mode changes
     the app sends a fresh `NoiseMode` (len 0, gains 0).
   * Mode values are QCY's `ancindex` from the panel (1 ANC on, 2 ANC off,
     3 transparency; one panel uses 65), **not** JieLi's usual 0/1/2
     **(uncertain how the firmware interprets them)**.
   * Reported to the UI as percent `(cur − min) × 100 / (max − min)`.

### 5.5 Equalizer

Panel `type 2`. Two flavours (`SysEQSeted.isSupportMultiEQ()` = panel has
`q` and `freq` lists):

**Graphic EQ (no `q` in the panel; older/cheaper models)**: RCSP sys info,
function `0xFF` (public), attribute type 4.

* Read: GET_SYS_INFO `0x07` params `FF 00 00 00 10` (mask bit 4)
  (`CommandBuilder.buildGetEqValueCmd`). Response params
  `FF | LTV(type 4: mode, g0…g9)`. The app throttles reads to one per 3 s.
* Write: SET_SYS_INFO `0x08` params `FF 0C 04 mode g0 … g9`
  (`CommandBuilder.buildSetEqValueCmd`, lines 97-113).
  QCY (`JLDeviceImpl.setEQ`, lines 576-608): custom curve → `mode = 0xFF`
  with the 10 gains as signed dB bytes; preset → `mode = preset index` and
  **all ten gains = 127**.
* Change notification: device command `0x09` (UPDATE_SYS_INFO) params
  `FF | LTV(type 4 …)` → `onEqChange` (`RcspDataHandler.parsePublicData`).
  QCY reports `[mode, gains…]`, zeros for mode 1.
* Bands are 31/62/125/250/500/1k/2k/4k/8k/16k Hz, range ±8 dB (from the
  default `res/raw/controlpan_jl.json`).

**Parametric EQ (panel has `q`/`freq`)**: QCY custom TLV, same layout as
WuQi (`docs/PROTOCOL.md` §7), but **at most 10 bands and no padding**:

* Panel has `iitType` → `setEQWtihQFG2` = id `0x22`, 7-byte bands
  `f LE16, gain×100 LE16, q×100 LE16, type`.
* No `iitType` → `setEQWtihQFG` = id `0x20`, 6-byte bands (no type).
* Header `id, len = 3 + bands×size, index, master×100 LE16`
  (`JLDeviceImpl.java:617-713`). Index 255/0 custom, 128, 126 (sound market),
  or preset id. Note a QCY bug: the buffer is sized for *all* bands but only
  the first 10 are filled, so extra bands become zeros.
* Left/right ear variants use ids `0x46`/`0x47` (7-byte bands).
* Read: custom `FE 01 20` and `FE 01 22` (`getEQInfoFQG`).

### 5.6 Touch keys

* New JL: custom `2B 02 gesture action` per change (`setLeftClick1…`,
  lines 361-487). Gesture numbering as QCY: 1/2 single L/R, 3/4 double,
  5/6 triple, 9/10 long. For other keys (`setBtnClickFunction`) only the low
  byte of the panel's event id is sent as gesture. Read: `FE 01 2B` → pairs.
* Old JL: SET_ADV_INFO attr **2** with one triplet `keyNum action function`:
  keyNum 1 = left, 2 = right (headphones: panel `eventID` = `key<<8 | action`,
  e.g. 769 = key 3 single, 1281 = key 5, 1793/1794 = key 7 single/double);
  action 1 single, 2 double, 3 triple, 4 quadruple **(4 inferred from the
  reverse mapping)**, 5 long press. `function` = QCY action id, but **0 (none)
  is sent as 127** and 127 is read back as 0 (`changeQCY2JL/changeJL2QCY`,
  lines 98-110). Read: GET_ADV_INFO mask −1, attr 2 = list of triplets; QCY
  maps (key 1, action a) → gesture `2a−1`, (key 2, action a) → `2a`, others →
  `key<<8 | action` (`updateSetting`, lines 1133-1175).
* QCY action ids (panel lists): 0 none, 1 play/pause, 2 previous, 3 next,
  4 voice assistant, 5 vol+, 6 vol−, 7 gaming mode, 11 (`0B`) ANC cycle.
  JieLi's own SDK reserves 255 for "switch ANC" (`KEY_FUNC_ID_SWITCH_ANC_MODE`);
  QCY panels use 11.

### 5.7 In-ear detection

* Simple switch (panel item type 201 or legacy char `0x000F`), **old and new**:
  SET_ADV_INFO attr **8** = `1` on / `2` off (`setInEarCheck`, line 715).
  Read: GET_ADV_INFO mask `0x0100` → attr 8 (`getCheckSwitch`, line 1029),
  `1` = on.
* Wear-detection settings page (QCY id `0x2C`): custom
  `2C 03 enabled music anc` (`PeidaiDataBean.getCMDBytes`), read `FE 01 2C`.

### 5.8 Gaming / low-latency mode

* New: custom `09 01 01` on / `09 01 02` off; read `FE 01 09`.
* Old: SET_ADV_INFO attr **5** (work mode) = `2` game / `1` normal. Read:
  attr 5 from GET_ADV_INFO (`workModel == 2`). (`getGameMode` for old devices
  asks mask `0x20` but then reads the sleep field: a QCY bug.)
* Target info attr 19 bit1 also reports game mode.
* Some newer panels use QCY id `0x4A` (game config, `[enable, sync sound]`)
  instead; that switch reboots the buds.

### 5.9 Sleep mode

New: custom `10 01 01/02`, read `FE 01 10`. Old: ADV attr **10** = `1`/`2`,
read mask `0x0400`.

### 5.10 Device name

New: custom `18 len utf8…` (`PairnameDataBean.getNameCmd`), read `FE 01 18`.
Old: SET_ADV_INFO attr **1** = name bytes (platform default charset, UTF-8);
result byte 2 = too long. The QCY app itself never reads the name on JL
(`requestPairName` returns the cached advert name), but old devices report
attr 1 in the `−1` ADV dump.

### 5.11 Channel balance

New: custom `16 01 v` (0..100), read `FE 01 16`. Old: ADV attr **12** =
`v` (0..100), read mask `0x1000`.

### 5.12 Prompt language

Panel item type 1001 (`cmdid 25`, `itemKey` list such as `4A4C3030` = "JL00"
English, `4A4C3031` = "JL01" Chinese) opens `SubStringItemListActivity`, which
calls `setBytesValue(25, key)` → custom `19 04 4A 4C 30 30`
(`v2ui/view/SingleKVItemView.java:237`, `SubStringItemListActivity.java:84`).
Read: `FE 01 19`. (`QCYConnectManager.setVoiceName`, which writes to A001, is
only used by the voice-pack download screen and is irrelevant here.) Old
devices also report the language as ADV attr 9 (string), cached by
`JLDeviceImpl.getLanuage()`.

### 5.13 Reset / clear pairing

| Action | New JL | Old JL |
|--------|--------|--------|
| restore settings | custom `01 00` | ADV_SYS_OP `0xC5` param `01` |
| clear pairing | custom `02 00` | `0xC5` param `02` |
| factory reset | custom `03 00` | custom `03 00` (no old branch) |

(`JLDeviceImpl.java:1220-1243`, triggered via legacy char `0x000A` values
1/2/3 in `QCYConnectManager.sendBleData`, lines 1336-1376.) `0xC5` response
params: `[op, result]` or `[result]`.

### 5.14 Other QCY ids used on JL via the custom command

Power-off timer `14 04 lo hi 00 00` (minutes LE16, **0 = power off now**),
standby `1F`, tone volume `1D`, fit test `11 03 x x x`, LED `35`/`36`,
AI ANC `54 06 …`, head gestures `50`/`51`, phone management `4D`, spatial
audio `5B`, hearing health `58`, dual device `24`, LDAC `23`, music control
`04`. All are byte-identical to the WuQi payloads in `docs/PROTOCOL.md`, minus
the `FF len` frame header.

### 5.15 Find device

Not a device command. The "Find headset" page (`ui/device/LocalMapActivity`)
plays an alarm on the phone through A2DP and shows the last location.

### 5.16 Device-originated events

| Opcode (bit7=1) | Handling |
|-----------------|----------|
| `0xFF` custom | QCY TLV, parse as §5.1 |
| `0x09` UPDATE_SYS_INFO | `function, LTV…`; fn `0xFF` type 1 volume, 4 EQ, 6 mode; fn 1 music status/name/play mode |
| `0xC2` ADV notify | 18+ bytes: `vid BE16, uid BE16, pid BE16, chip<<4\|ver, edr addr[6], action, L, R, case (bit7 charging), seq`. Only sent after `0xC3 01` (never sent by QCY). |
| `0xC4` request ADV op | param `op`: 0 → re-read ADV info (mask −1; old QCY devices also run `updateSetting`), 1 → "need reboot" (ignore), 2 → resend time (attr 7) |

## 6. ADV attributes (`0xC0` set, `0xC1` get)

`com/jieli/jl_bt_rcsp/constant/AttrAndFunCode.java` (`ADV_TYPE_*`),
`ParseHelper.parseADVInfo` (lines 640-730), `TwsCmdHandler`.

* **Set** `0xC0`: params = one LTV `len type data…`
  (`CommandBuilder.buildSetAdvInfoCmd` → `ParseDataUtil.packLTVPacket`).
  Response params `[result]`: 0 OK, 1 refused while in game mode, 2 name too
  long, 3 LED setting failed (`StateCode.ADV_SETTINGS_ERROR_*`).
* **Get** `0xC1`: params = `mask BE32`, bit *n* = attribute type *n*
  (`-1` = all). Response params = LTV list.

| Type | Name | Data | QCY use |
|------|------|------|---------|
| 0 | battery | `L, [R, [case]]`, bit7 charging, bits 0-6 % | old battery |
| 1 | device name | bytes | old rename |
| 2 | key settings | `{keyNum, action, function}` × n | old keys (§5.6) |
| 3 | LED settings | `{scene, effect}` × n | not used by QCY |
| 4 | mic channel | 1 byte | not used |
| 5 | work mode | 1 normal, 2 game (low latency) | old game mode |
| 6 | product info | `vid BE16, uid BE16, pid BE16` | — |
| 7 | connected time | set only: `u32 BE` unix seconds | sent on connect |
| 8 | in-ear detection | 1 on, 2 off | old **and** new |
| 9 | language | ASCII | read only |
| 10 | sleep mode | 1 on, 2 off | old |
| 11 | noise mode | `mode, len, min BE16, max BE16, cur BE16` | percent-ANC panels |
| 12 | volume balance | 0..100 | old |

Masks QCY uses: `0x0D21` (battery poll), `0x0100` in-ear, `0x0400` sleep,
`0x0020` work mode, `0x0800` noise mode, `0x1000` balance, `0xFFFFFFFF` all.

Example, old JL "left double tap → next track":
`FE DC BA C0 C0 00 06 sn 04 02 01 02 03 EF`.

Other ADV opcodes: `0xC2` device notify (§5.16), `0xC3` enable/disable
notify (`[1]`/`[0]`), `0xC4` device request (§5.16), `0xC5` system op
(1 restore settings, 2 clear pairing, 3 "reset"; QCY uses 1 and 2 only on old
devices).

## 7. Old vs new JieLi devices

`isNewJLDevice()` = `(vendorId & 0x80F0) == 0x8070`. In the downloaded
panels these are the MeloBuds N20/N30/N65/A30, T13 Plus/Pro, HT16/HT20,
Buds ANC, Crossky C50i (32881–32894) and T13 PRO ANC BT6 (36977). All other
`0x?7?` ids are "old".

| Feature | Old (`isNewJLDevice == false`) | New |
|---------|-------------------------------|-----|
| Keys write | ADV attr 2 `[key, action, fn (0→127)]` | custom `2B 02 g a` |
| Keys/battery/etc. read | GET_ADV mask −1 (`updateSetting`) | custom reads `2B, 2F, 10, 09, 18` (10 ms apart) |
| Battery | ADV mask `0x0D21` | custom `2F` |
| Version | target info attr 5 | custom `30` |
| Sleep mode | ADV attr 10 | custom `10` |
| Game mode | ADV attr 5 (2 = on) | custom `09` (1 = on) |
| Rename | ADV attr 1 | custom `18` |
| Balance | ADV attr 12 | custom `16` |
| Restore / clear pairing | `0xC5` op 1 / 2 | custom `01 00` / `02 00` |
| Device request `0xC4` | re-reads all settings | ignored by QCY (SDK still re-reads ADV) |
| Same on both | in-ear (ADV 8), graphic EQ (sys info), parametric EQ / ANC scenes / all other QCY ids (custom), factory reset (custom `03 00`), time sync (ADV 7) |

Model-specific branches seen in the JL code paths:

* 16758 (H3 Lite): ANC writes throttled to one per 3 s.
* 16753/16754/16756 (`isH2`): EQ re-read on every battery poll.
* `checkCanNotify()` list (§1.1): ANC polled every 3 s.
* JL always caps parametric EQ at 10 bands (no `0x44` query result used).

## 8. Opcodes to never send

Blacklist for the JL transport (A002/0x0001). Everything in the first group
belongs to firmware update; the second group can reboot or wipe the device.

| Opcode | Name (JieLi SDK) | Why |
|--------|------------------|-----|
| `0xE1` (225) | OTA get update file info offset | firmware update |
| `0xE2` (226) | OTA inquire if can update | firmware update |
| `0xE3` (227) | OTA enter update mode | firmware update |
| `0xE4` (228) | OTA exit update mode | firmware update |
| `0xE5` (229) | OTA firmware block (device → phone request) | firmware update |
| `0xE6` (230) | OTA get refresh-firmware status | firmware update |
| `0xE8` (232) | OTA notify update content size | firmware update |
| `0xD4` (212) | get device MD5 | only used for update checks |
| `0x01` | DATA (xmOpCode sub-protocol) | QCY never uses it; also carries file/update data in other SDKs |
| `0x16`–`0x1E`, `0x22`–`0x24`, `0x28` | file / large-file transfer, format, delete (jl_rcsp only) | can write flash |
| `0xE7` (231) | reboot device (param 0 reboot, 1 power off) | reboots; also used by the update flow |
| `0xC5` op `0x03` | ADV system op "reset" | **(uncertain)** reset/reboot |
| `0x06` | disconnect classic BT / auth-flag reset | drops the audio link |
| `0xD1` (209) | set communication MTU | not needed; QCY never sends it |

Also never act on target info attributes 8/9 (mandatory upgrade flags), and
never connect to a device advertising `0x05D6` data starting with `"ATOLJ"`
or with a model id where `isOTAOnlyDevice()` is true.

QCY TLV ids inside the custom command that need care (not firmware, but
destructive): `0x03` factory reset, `0x02` clear pairing, `0x14` with 0
minutes (powers off immediately), `0x23`/`0x24` (reboot).

## 9. Open questions (need a real device / capture)

1. Which characteristics JL QCY earbuds expose on service A001, and whether
   A001 notifications (e.g. battery `0x0008`) work on them.
2. Write type of A002/0x0001 (with or without response).
3. Whether the firmware ever demands the §3.2 auth on A002 (code says no).
4. How the firmware interprets JieLi NoiseMode `mode` values 1/2/3/65 (§5.4).
5. Exact content of the custom-command write *response* (probably empty).
6. Whether frames larger than `MTU − 3` may be split across writes.

## 10. Implementation checklist

1. Scan for `0x521C` adverts, decode the model id, `isJL = (id & 0xF0) == 0x70`
   (skip 512 and `isOTAOnlyDevice`), `isNew = (id & 0x80F0) == 0x8070`.
2. Connect GATT, negotiate MTU, subscribe to A002/0x0002.
3. Send GET_TARGET_INFO (`03`, `FF FF FF FF 00`), then SET_ADV attr 7 (time).
4. Run a stream parser (§2.3) and a request table keyed by (opcode, sn) with
   2 s timeout and 3 resends.
5. For settings use the custom command (`0xFF`) with QCY TLVs; for old models
   use ADV attributes for keys/sleep/game/name/balance/battery as in §7.
6. Poll battery (custom `2F` or ADV mask `0x0D21`) every 30 s; poll `0x17`
   every 3 s for `!checkCanNotify()` models; handle device commands `0xFF`,
   `0x09`, `0xC4` without replying.
7. Reuse the per-model panel JSON (`research/panel_json/models/*.json`, 47
   JieLi models, chip `"JieLi"`) for capabilities, exactly as for WuQi.
8. Refuse to send any opcode from §8.
