# Web research: JieLi RCSP and QCY earbud protocols

Compiled 2026-10-13. Research only: no Bluetooth device was contacted. Firmware
update / OTA material was deliberately skipped. Opcodes in the OTA range are only
listed so the app can avoid them.

Local copies of the sources (text and headers only, no binaries) are in
`research/web_sources/`. The clean-room check script is `research/tools/bt_e1_check.py`.

Reliability scale used below:
- **A**: official JieLi source code or documentation, or verified here by running code.
- **B**: an independent open-source implementation that reports testing on real hardware.
- **C**: a community write-up or a single-device observation that has not been confirmed elsewhere.

---

## 1. RCSP authentication (most important)

### 1.1 Summary

**The RCSP authentication can be implemented from public information. It does not need `libjl_auth.so`.**

- The handshake is a mutual challenge/response that uses 16-byte blocks. Its messages are sent raw, without RCSP framing.
- The cipher is the **Bluetooth Core Specification E1 authentication function**. E1 is built on SAFER+, and JieLi uses the full 128-bit output.
- The inputs are:
  - **key** = `06 77 5f 87 91 8d d4 23 00 5d f1 d8 cf 0c 14 2b`
  - **address** = `11 22 33 33 22 11`
- I wrote a clean-room E1 from the spec structure (`research/tools/bt_e1_check.py`). It reproduces all three known-answer vectors captured from real JieLi devices (see 1.4). Reliability **A**.

### 1.2 Handshake (phone = host)

The handshake runs on AE01 (write) and AE02 (notify) over BLE, or as raw bytes on the RFCOMM/SPP channel. Every message is unframed: there is no `FE DC BA` header and no `EF` end byte.

```
host -> dev   00 || R_h (16 random bytes)
dev  -> host  01 || E1(key, R_h, addr)          host must verify this
host -> dev   02 'p' 'a' 's' 's'                  (0x02 0x70 0x61 0x73 0x73)
dev  -> host  00 || R_d (16 random bytes)
host -> dev   01 || E1(key, R_d, addr)
dev  -> host  02 'p' 'a' 's' 's'                  authenticated
```

Until authentication passes, the firmware hands every received byte to `JL_rcsp_auth_recieve()` and ignores RCSP frames.

Sources:
- https://github.com/ElectronicCats/jieli-ble-badge-research/blob/main/tools/qix-ble/qix_ble/auth.py (MIT). Six-step state machine with KATs. **B**
- https://github.com/hybridherbst/web-bluetooth-e87 (README, "Authentication Handshake"; MIT). **B**
- https://github.com/Desmond123-arch/spacebuds-ctl/blob/main/docs/protocol.md (Apache-2.0). The same handshake on JieLi AC69xx *earbuds* (Oraimo Spacebuds Neo) over RFCOMM channel 1. **B**
- JieLi firmware SDK, device side. Source: https://github.com/Jieli-Tech/fw-AC63_BT_SDK/blob/master/apps/common/third_party_profile/jieli/JL_rcsp/rcsp_bluetooth.c (Apache-2.0). `JL_rcsp_recieve_handle()` calls `JL_rcsp_auth_recieve()` until `JL_rcsp_get_auth_flag()` is set. **A**

### 1.3 Key, address and algorithm sources

- **Link key, from JieLi's own Apache-2.0 firmware SDK.**
  - The key is `static const u8 link_key_data[16] = {0x06, 0x77, 0x5f, 0x87, 0x91, 0x8d, 0xd4, 0x23, 0x00, 0x5d, 0xf1, 0xd8, 0xcf, 0x0c, 0x14, 0x2b};`
  - It is passed as `JL_rcsp_auth_init(send_fn, link_key_data, NULL)` for BLE and for SPP.
  - The same key is in `fw-AW31N_BLE_SDK` (`apps/app/bsp/common/third_party_profile/jieli/JL_rcsp/rcsp_bluetooth.c`).
  - The algorithm itself is inside the closed library `cpu/*/liba/rcsp_stack.a`.
  - Reliability **A**.
- **JieLi's own published host-side cipher.**
  - JieLi publishes it as JavaScript in https://github.com/Jieli-Tech/WeChat-Mini-Program-OTA, files `libs/jl_auth_2.0.0.js` and `jl_auth_2.0.0.d.ts`. The repository is **Apache-2.0**. Its header reads "杰理认证库 version 2.0".
  - The API is `Auth.startAuth(deviceId, listener)`, `Auth.handlerAuth(deviceId, data)` and `setLinkKey(newLinkKey)`.
  - The minified code starts with `[17,34,51,51,34,17]` (the address/"seed") and `[6,119,95,135,…,20,43]` (the link key).
  - This repository is about OTA. Only the auth file was looked at, and nothing about OTA was read.
  - Reliability **A**.
- **`setLinkKey()` exists.** This means a vendor can replace the default key. QCY may use the default key or a custom one. **To confirm this, check QCY's `libjl_auth.so` / `com.jieli.jl_rcsp` code in `research/decompiled/` for a different 16-byte key.** This is an open question and has not been checked.
- **Firmware build flag that skips auth.**
  - In `rcsp_bluetooth.c`, the receive handler skips authentication when `BT_CONNECTION_VERIFY != 0`, and `rcsp_init()` then calls `JL_rcsp_set_auth_flag(1)`.
  - So some products may ship with authentication disabled.
  - The Android SDK also has `BluetoothOption.setUseDeviceAuth(bool)`, described as "confirm with firmware engineer": https://github.com/Jieli-Tech/Android-JL_Bluetooth/blob/main/README_en.md
  - Reliability **A**.

### 1.4 Why it is E1 (SAFER+)

Read from the reverse-engineered tables and logic in `qix_ble/auth.py`:

- `SBOX` is the SAFER+ exponent table `45^i mod 257`: `01 2d e2 93 …`. `ISBOX` is its logarithm.
- `KS_TABLE` holds the SAFER+ key-schedule bias bytes.
- The "Fibonacci butterfly" is the SAFER+ PHT layer combined with the Armenian shuffle.
- The XOR/ADD mask `0x9999` is the SAFER+ byte pattern for positions 0, 3, 4, 7, 8, 11, 12 and 15.
- The "mode != 0 and round == 2: mix initial input" step is the `Ar'` modification of E1.
- The seed offsets `-0x17, ^0xE5, -0x21, ^0xC1, …` are E1's K̃ derivation: add 233, xor 229, add 223, xor 193, and so on.
- `output = Ar'(K̃, (Ar(K, RAND) ^ RAND) + E(addr))`, where `E(addr)` repeats the 6-byte address cyclically to 16 bytes.

Clean-room check (`python3 -I research/tools/bt_e1_check.py`). Every vector prints OK:

| RAND (challenge) | expected E1 output (= device response) | source of the vector |
|---|---|---|
| `b6e080ecaff322916d88fad5aa34c2ac` | `1d8897ac4604d332e8175e81bb292524` | hybridherbst jl_auth_v3.py KAT |
| `a53057d376fc0476e5dc68273b5e791e` | `b3242d82e1df6c545895ecae13b44fea` | ElectronicCats E87 capture |
| `384ac9d8e481e5caaeb8cead20ee8f9e` | `fe1b5b643a790fdcf585e354dd377b0f` | ElectronicCats E87 capture |

- I did not cross-check the implementation against the Bluetooth spec's own E1 sample data. It matches the device vectors, which is what matters here.
- A Rust implementation is about 120 lines: SAFER+ tables computed at startup, key schedule, `Ar` / `Ar'`, and E1. It can be written from the Bluetooth Core Spec, Vol 2 Part H §6 "Authentication function E1", so no third-party code needs to be copied.

### 1.5 Other open implementations of the auth (for cross-checking)

| Project | Language | Licence | Notes |
|---|---|---|---|
| https://github.com/Desmond123-arch/spacebuds-ctl (`jl_auth.py`) | Python | Apache-2.0 | Port of JieLi's `jl_auth_2.0.0.js`; verified against an earbud capture |
| https://github.com/ElectronicCats/jieli-ble-badge-research (`tools/qix-ble/qix_ble/auth.py`, tests) | Python | MIT | Disassembly-derived; three KATs |
| https://github.com/hybridherbst/web-bluetooth-e87 (`protocol-understanding/jl_auth*.py`, `web/src/jl-auth.ts`) | Python/TS | MIT | Origin of the RE port |
| https://github.com/Manaiakalani/auracast (`web/src/jl-auth.ts`, `relay/auracast_relay/auth.py`) | TS/Python | MIT | Badge app |
| https://github.com/guan4tou2/ebadge-python-cli (`ebadge_cli/jl_auth.py`) | Python | MIT | Badge CLI |
| https://github.com/KristianP26/ble-scale-sync (`src/scales/jieli-auth.ts`) | TS | GPL-3.0 | JieLi-based smart scales |
| https://github.com/virtualabs/homday-xpert-smartwatch (`watchfaces/upload/auth.py`) | Python | GPL-3.0 | JieLi smartwatch |
| https://github.com/killsng/RedmiBudsControl (`docs/AUTH_CIPHER.md`) | doc | none | Redmi buds (macOS) |
| https://github.com/HBandSDK/WeChat_Mini_Program_Ble_SDK (`libs/jieli_sdk/lib/rcsp-impl/rcsp.ts`) | TS | Apache-2.0 | Vendor SDK that bundles JieLi RCSP |

There is no JieLi or RCSP support in Gadgetbridge. Searches of the Gadgetbridge sources on Codeberg and on the GitHub mirror for "jieli", "rcsp" and "qcy" found nothing.

---

## 2. RCSP framing and transport

### 2.1 Transport

- **BLE GATT.** Service `0000ae00-0000-1000-8000-00805f9b34fb`:
  - `ae01` is Write Without Response (host to device).
  - `ae02` is Notify (device to host). Enable its CCCD.
- Sources:
  - JL701N earphone SDK GATT table `le_rcsp_adv_module.h`, mirrored at https://github.com/Buddie-official/Jieli-Development-Board (`sdk/apps/common/third_party_profile/jieli/JL_rcsp/bt_trans_data/le_rcsp_adv_module.h`). The mirror has **no licence**, and it is the JieLi SDK republished by a third party.
  - Confirmed on devices by hybridherbst and ElectronicCats.
  - Reliability **A/B**.
- The UUIDs are configurable in the SDK (`BluetoothOption.setBleUUID(service, write, notify)`), so a vendor could change them. Source: Android-JL_Bluetooth README. **A**
- **SPP / RFCOMM.** RCSP also runs over classic Bluetooth SPP (`BluetoothOption.PREFER_SPP`, `setSppUUID`). On Oraimo buds it is RFCOMM channel 1, and HFP is on channel 4. Source: spacebuds-ctl, reliability **B**. I have not yet confirmed the default SPP UUID value from a public source.
- **MTU.** The firmware defaults are `JL_MTU_RESV = 540` and `JL_MTU_SEND = 128`. Opcode `0xD1` (NOTIFY_MTU) negotiates it, and target-info attribute 13 (`DEV_MAX_MTU`) reports it. Source: `JL_rcsp_packet.h`, reliability **A**.

### 2.2 Frame format

Source: `include_lib/btstack/third_party/rcsp/JL_rcsp_packet.h` in https://github.com/Jieli-Tech/fw-AC63_BT_SDK (Apache-2.0). Reliability **A**.

```
FE DC BA | HEAD (2 bytes) | LEN (2 bytes, big-endian) | DATA (LEN bytes) | EF
```

- **HEAD** is a 16-bit field with these bits:
  - bit 15: type (1 = command, 0 = response)
  - bit 14: response requested
  - bits 13..8: unused
  - bits 7..0: opcode
- **HEAD on the wire.** Captures show the flag byte first and then the opcode, for example `FE DC BA C0 03 …`. So the 16-bit HEAD is sent big-endian.
  - Seen on earbuds (spacebuds-ctl) and on the E87 badge (hybridherbst). Reliability **B**.
  - The note at https://github.com/yschimke/cadence/blob/main/research/rcsp.md says `[op, flags]`. That is **wrong**, and the same note also mislabels the repos as MIT. Reliability **C**.
- **Flag byte values:**
  - `C0`: command that wants a response
  - `80`: command or notification with no response wanted
  - `00`: response
  - `40` would mean a response that requests a response. It is unused.
- **Command DATA** is `SN (1 byte) || payload`.
- **Response DATA** is `STATUS (1 byte) || SN || payload`.
- There is no checksum.
- The `DATA` opcode (`0x01`) carries an extra "CMD_OpCode" byte. See `JL_DATA_send(OpCode, CMD_OpCode, …)` in `JL_rcsp_protocol.h`.
- **Status codes** (`JL_PRO_STATUS`):

  | Code | Meaning |
  |---|---|
  | 0 | success |
  | 1 | fail |
  | 2 | unknown command |
  | 3 | busy |
  | 4 | no response |
  | 5 | CRC error |
  | 6 | all-data CRC error |
  | 7 | parameter error |
  | 8 | response data over limit |

- **LTV attribute records.** Many payloads are sequences of `LEN (1) | TYPE (1) | VALUE (LEN-1)`. Source: https://doc.zh-jieli.com/Apps/Android/jielihome/en-us/master/development/function/terminology_note.html, reliability **A**.
- **Masks** in get requests are 32-bit **big-endian** bitmaps, read with `READ_BIG_U32(data)`. Bit n requests attribute type n.

---

## 3. RCSP opcode and attribute reference

### 3.1 Sources (best first)

1. **`JL_OpCode.h`** in the JieLi iOS SDK, https://github.com/Jieli-Tech/iOS-JL_Bluetooth (the LICENSE file is Apache-2.0; GitHub reports it as NOASSERTION).
   - Path: `code/JieLi_Home_Demo/Frameworks/JL_BLEKit.xcframework/ios-arm64/JL_BLEKit.framework/Headers/JL_OpCode.h`
   - This is the most complete host-side opcode list. Reliability **A**.
2. **`JL_rcsp_protocol.h`** in fw-AC63_BT_SDK (Apache-2.0). It gives the opcode enum, the target-info attribute IDs and the common sys-info attribute IDs. Reliability **A**.
3. **JL701N earphone SDK**, mirrored without a licence at https://github.com/Buddie-official/Jieli-Development-Board:
   - `le_rcsp_adv_module.h` holds the ADV opcodes and attributes.
   - `rcsp_adv_bluetooth.c` holds the request and response handlers.
   - `adv_anc_voice.c` holds the ANC payload.
   - These files show the exact earbud payload layouts. **Read them; do not copy them**, because they are a third-party mirror with no licence. Reliability **A** for content, but the licence is unclear.
4. **JieLi Home SDK docs** (English), https://doc.zh-jieli.com/Apps/Android/jielihome/en-us/master/index.html. The constants page is https://doc.zh-jieli.com/Apps/Android/jielihome/en-us/master/development/constant/func_code.html. A local text dump is in `research/web_sources/jieli_home_sdk_docs_en/`. Reliability **A**.

### 3.2 Opcodes relevant to an earbud controller

| Op | Name in the JieLi headers | Use |
|---|---|---|
| 0x01 | `DATA` / `kJL_DATA_CMD` | data carrier that includes a sub-opcode |
| 0x03 | `GET_TARGET_FEATURE` | device and target info. Payload is a big-endian u32 mask; reply is LTV records. The app sends it first. |
| 0x06 | `DISCONNECT_EDR` | |
| 0x07 | `SYS_INFO_GET` | payload `function (1) or mask (u32 BE)`; reply `function` plus LTV records |
| 0x08 | `SYS_INFO_SET` | payload `function` plus LTV records |
| 0x09 | `SYS_INFO_AUTO_UPDATE` | device pushes changes in the same layout as the 0x07 reply (flag 0x80, no response) |
| 0x0E | `FUNCTION_CMD` | mode-specific operations (music play/pause and similar) |
| 0x19 | `FIND_DEVICE` | find earbuds / find phone |
| 0x33 | `kJL_PUBLIC_SET` | newer "public settings" command; not studied |
| 0xC0 | `SET_ADV` | write earbud "ADV" settings (LTV records) |
| 0xC1 | `GET_ADV` | read ADV settings. Payload is a big-endian u32 mask; reply is LTV records. |
| 0xC2 | `ADV_DEVICE_NOTIFY` | device pushes ADV settings when they change |
| 0xC3 | `ADV_NOTIFY_SETTING` | 1 byte that enables or disables the 0xC2 pushes |
| 0xC4 | `ADV_DEVICE_REQUEST` | device asks the host to act, e.g. update config or sync time |
| 0xD0 | `SET_APP_INFO` | |
| 0xD1 | `NOTIFY_MTU` | |
| 0xFF | `CUSTOMER_USER` | vendor-defined custom command. `rcsp_user_recv_cmd_resp(data,len)` handles it in firmware; the SDK sends it with `CommandBuilder.buildCustomCmd(byte[])`. |
| 0xE1–0xE8, 0x0B, 0x1A–0x28 | OTA, flash and file-transfer range | **Never send these** (hard rule). Also avoid `JL_OPCODE_SET_DEVICE_REBOOT`, which sits in the update opcode range in the JL701N source. |

### 3.3 Attributes for `GET_TARGET_FEATURE` (0x03)

Source: `JL_rcsp_protocol.h`.

| ID | Attribute |
|---|---|
| 0 | protocol version |
| 1 | sys info |
| 2 | EDR address |
| 3 | platform |
| 4 | function info |
| 5 | device version |
| 6 | SDK type |
| 7 | uboot version |
| 8 | dual partition (OTA, ignore) |
| 9 | update status (OTA, ignore) |
| 10 | VID/PID |
| 11 | auth key (a server-verification string, not the link key) |
| 12 | project code |
| 13 | max MTU |
| 17 | BLE address |
| 19 | MD5 / game support flags |

### 3.4 Sys-info attributes (opcodes 0x07, 0x08, 0x09)

Function byte 0xFF is "public/common"; 0 is BT mode.

Common attribute IDs, from `JL_rcsp_protocol.h` and the docs page `func_code.html` (4.1.1):

| ID | Attribute |
|---|---|
| 0 | battery |
| 1 | volume |
| 4 | EQ |
| 11 | treble/bass |
| 12 | EQ preset info |
| 13 (0x0D) | current noise mode (ANC) |
| 14 (0x0E) | all noise modes |
| 15 | phone call state |
| 16 | extended functions |
| 21 (0x15) | adaptive ANC |
| 22 | smart no-pick |
| 23 | scene denoise |
| 24 | wind-noise detection |
| 25 | vocal booster |
| 27 (0x1B) | dual connection |

**ANC payload** for attribute 13 (from JL701N `adv_anc_voice.c` and the docs' `VoiceMode` class):

- `mode (1)` then `left_max (s16 BE)`, `right_max (s16 BE)`, `left_cur (s16 BE)`, `right_cur (s16 BE)`.
- Mode values: 0 = normal/off, 1 = noise cancelling, 2 = transparency. These are `VOICE_MODE_CLOSE`, `VOICE_MODE_DENOISE` and `VOICE_MODE_TRANSPARENT`.
- Set it with `SYS_INFO_SET`, function 0xFF, LTV type 13.
- Attribute 14 returns `count` followed by `count` × (`mode` + 8 bytes).
- The firmware switches ANC with `anc_mode_switch(mode+1)`.

Reliability: **A** for the field layout. Whether QCY's JieLi firmware enables `RCSP_ADV_ANC_VOICE` is **unknown**.

### 3.5 ADV attributes (opcodes 0xC0, 0xC1, 0xC2)

Sources: `le_rcsp_adv_module.h`, and the docs' `AttrAndFunCode.ADV_TYPE_*` (func_code.html, section 4.3).

| ID | Firmware name | SDK name | Payload |
|---|---|---|---|
| 0 | `BAT_VALUE` | `ADV_TYPE_BATTERY_QUANTITY` | 3 bytes: left, right, case. Bit 7 = charging, bits 0–6 = percent. Confirmed on Oraimo buds as `64 64 00`. |
| 1 | `EDR_NAME` | `ADV_TYPE_DEVICE_NAME` | UTF-8 name, at most 20 bytes |
| 2 | `KEY_SETTING` | `ADV_TYPE_KEY_SETTINGS` | triples `[key_num, action, function]`. Firmware default is `01 01 05, 02 01 05, 01 02 08, 02 02 08`. |
| 3 | `LED_SETTING` | | pairs `[scene, effect]` |
| 4 | `MIC_SETTING` | | 1 byte: 1 auto, 2 always left, 3 always right |
| 5 | `WORK_MODE` | | 1 byte: 1 normal, 2 gaming (low latency) |
| 6 | `PRODUCT_MESSAGE` | | 6 bytes, vid/pid/uid |
| 7 | `TIME_STAMP` | | connected time, u32 BE |
| 8 | `EQ_SETTING` / `IN_EAR_CHECK` (SDK) | | the numbering differs between SDK generations (see below) |
| 9 | | `ADV_TYPE_LANGUAGE` | |
| 10 | `ANC_VOICE_KEY` | `ADV_TYPE_ANC_MODE_LIST` | modes that the ANC key cycles through. The ANC-switch key function id is 0xFF. |

Notes on these attributes:

- **Key settings.** `key_num` is 1 for left and 2 for right. `action` is 1 single tap, 2 double tap, 3 triple tap. Function codes from the public JieLi config `ac697x_headset_json.txt`: 0 none, 3 previous, 4 next, 5 play/pause, 6 answer, 7 hang up, 8 call back, 9 volume up, 10 volume down. On Oraimo buds (spacebuds-ctl) a write is `C0` with body `sn 13 02 <18 bytes>`, that is all six triples in one record.
- **Numbering differs between generations.** The firmware header and the Android SDK constants disagree for IDs at or above 8. In the firmware, 8 = EQ, 9 = high/low volume, 10 = ANC key, 11 = ANC, 12 = assisted hearing. The Android docs list 8 = in-ear check, 9 = language, 10 = ANC mode list. **Probe with the 0xC1 mask before writing.**
- **Mask byte order.** The 0xC1 request mask is a u32 **BE**. spacebuds-ctl sends `ff ff ff ff`.
- **Write reply.** The response to a 0xC0 write is status 0 plus 1 byte, where 0 = ok and 1 = rejected.

### 3.6 Key-function configuration file from JieLi

https://github.com/Jieli-Tech/Android-JL_Bluetooth/tree/main/code/PiHome_V1.13.0_SDK_V4.2.0/btsmart/src/main/assets (Apache-2.0):

- `ac697x_headset_json.txt` and `ac693x_headset_json.txt` hold the key, LED, work-mode and mic-channel value tables. A local copy is in `research/web_sources/jieli_home_assets/`.

---

## 4. JieLi Android SDK API and how it maps to the protocol

- SDK: `jl_bluetooth_rcsp_V4.2.0_*.aar` (a binary) plus `jldecryption_*.aar`, in https://github.com/Jieli-Tech/Android-JL_Bluetooth.
  - The whole repository is **Apache-2.0** (copyright Zhuhai JieLi 2024). That includes the aar binaries, which allows inspecting them for interoperability.
  - **I did not download them.**
- Docs: https://doc.zh-jieli.com/Apps/Android/jielihome/en-us/master/ (Chinese at `/zh-cn/`). API pages:
  - `development/api/rcsp_basic_api.html`
  - `development/api/bt_api.html`
  - `development/function/*.html`
- Entry point: `RCSPController.init(context, BluetoothOption)`, then `RCSPController.getInstance()`. Event callbacks go through `BTRcspEventCallback`. The older entry point is `JL_BluetoothManager`.
- `BluetoothOption` fields: `priority` (PREFER_BLE / PREFER_SPP), `setUseDeviceAuth`, `setBleUUID(service, write, notify)`, `setSppUUID`, `mtu`, `bleScanStrategy` (NONE / ALL / FLAG / HASH filter), and `cmdSnGenerator`.

| SDK call | Wire mapping (from the docs and headers) |
|---|---|
| `getDeviceInfo` / `requestDeviceInfo(dev, mask)` → `TargetInfoResponse` | 0x03 with a mask |
| `getDevSysInfo` / `setDevSysInfo` | 0x07 / 0x08 |
| `getDeviceSettingsInfo(dev, mask)` → `ADVInfoResponse` (pid, vid, uid, battery L/R/case plus charging flags, `KeySettings{keyNum, action, function}`, `LedSettings{scene, effect}`, in-ear, language, ANC mode list) | 0xC1 |
| `modifyDeviceSettingsInfo`, `configKeySettings(dev, List<KeySettings>)`, `updateFunctionValue` | 0xC0 |
| `getAllVoiceModes` / `getCurrentVoiceMode` / `setCurrentVoiceMode(VoiceMode)` | 0x07 / 0x08 with function 0xFF and attribute 14 / 13 |
| `getEqInfo`, `setHighAndBassValue`, `getExpandDataInfo` | 0x07 / 0x08, attributes 4 / 11 / 16 |
| `startAdaptiveANC`, `setSmartNoPickParam`, `setSceneDenoising`, `setWindNoiseDetection`, `setVocalBooster` | attributes 21–25 |
| `queryDoubleConnectionState` / `setDoubleConnectionState` | attribute 27 |
| `CommandBuilder.buildCustomCmd(byte[])` + `sendRcspCommand` | 0xFF; the reply comes back as `CustomResponse.getData()` |
| `rebootDevice` | update-range opcode, so avoid it |

- Error codes `SUB_ERR_START_AUTH_DEVICE` and `SUB_ERR_AUTH_DEVICE` exist. See `development/constant/error_code.html`.
- **Caution.** The 0x07 / 0x08 path and the 0xC0 / 0xC1 ("ADV") path both carry earbud settings. TWS earbud firmware (the AC697x / JL701N "ADV" profile) uses 0xC0 / 0xC1 for keys, name, LED and work mode, and uses 0x07 / 0x08 attribute 13 for ANC.

---

## 5. Existing open-source projects

### 5.1 JieLi RCSP clients

| Project | Licence | What it does | Relevance |
|---|---|---|---|
| https://github.com/Desmond123-arch/spacebuds-ctl | Apache-2.0 | Python CLI/TUI for **Oraimo Spacebuds Neo (JieLi AC69xx earbuds)**: auth, battery (ADV attr 0), tap gestures (ADV attr 2) over RFCOMM channel 1 | **Closest match for JieLi earbuds**; small and readable |
| https://github.com/ElectronicCats/jieli-ble-badge-research | MIT | E87 badge (AC707N): BLE AE00 client, auth with tests, RCSP session | Best-tested auth plus framing |
| https://github.com/hybridherbst/web-bluetooth-e87 | MIT | Web Bluetooth badge writer; protocol README | Auth origin; good write-up |
| https://github.com/Manaiakalani/auracast, https://github.com/a968736/auracast-extended | MIT | Badge web apps | TypeScript auth |
| https://github.com/KristianP26/ble-scale-sync | GPL-3.0 | Includes JieLi scales | GPL, read only |
| https://github.com/yschimke/cadence (`research/rcsp.md`) | none | Opcode notes | **C**; contains the HEAD byte-order error |
| https://github.com/moudabdelwahab/oraimo (`docs/jieli-rcsp-apk.md`) | none | Notes from an Oraimo APK | not reviewed in depth |
| https://github.com/killsng/RedmiBudsControl | none | macOS app for Redmi buds; `docs/AUTH_CIPHER.md` | Redmi buds may use JieLi auth too |

Gadgetbridge has no JieLi, RCSP or QCY support (codeberg.org/Freeyourgadget/Gadgetbridge).

### 5.2 QCY protocol (0xA001) clients

The QCY app's own protocol, used by non-JieLi QCY models.

| Project | Licence | Platform / lang | Models / features |
|---|---|---|---|
| https://github.com/hui1601/Quicky | **none** (all rights reserved) | Go library, Linux/BlueZ | Main public protocol write-up: `docs/protocol.md` (40+ commands), `docs/service.md`, an embedded product DB of 199 models from QCY's API, and `scripts/dump_products.py`. Validated on HT07. Read it as documentation; do not copy code. |
| https://github.com/allansomensi/qcyx | MIT | **Rust** CLI + GUI | MeloBuds Pro: ANC, battery, EQ presets + custom EQ, balance, prompt volume, timers, wear detection, game, sleep, taps, LDAC, multipoint, rename, reset, RSSI, profiles. **The most useful Rust reference.** |
| https://github.com/pedro-labsabs/521C | MIT | Rust (+ TS), Linux | MeloBuds Pro HT08: battery, firmware, ANC/transparency, game mode, EQ, taps, wear, sleep, find; capability model |
| https://github.com/GiorgioRafael/OpenQCY-Desktop | MIT | C#/WinUI 3, Windows | MeloBuds N70 (HT18, vendor 23877): ANC + 5 scenes, EQ + custom 10-band (parametric), taps, wear, LDAC, multipoint, game, sleep, wind, prompt volume, auto-off |
| https://github.com/Pedromomas/OpenQCY-HT05 | MIT | C#, Windows | HT05 MeloBuds ANC (fw 5.0.3): ANC, transparency 1–6, game, 6 EQ presets, gestures, balance, sleep |
| https://github.com/Leandro-74/MelobudsNext | MIT | Python TUI, Linux | MeloBuds Pro fw 2.0.6: ANC scenes (cmd 0x17 `[mode, sub, noise]`), transparency, game (0x09), rename (0x18), state query (0xFE), battery char `…0008`, touch char `…000D` |
| https://github.com/HttpKiwi/OpenQCY | NOASSERTION | Flutter, Android | Reimplements Quicky's documented protocol |
| https://github.com/brain1401/qcy-eq | none | Go, Linux | Applies AutoEQ parametric presets to the N70 |
| https://github.com/tutleblue/wireless-limitless | none | Python/WebView, Windows | HT08 MeloBuds Pro (Plus) fw 2.0.7; full feature set |
| https://github.com/SouthautumnYa/QCYpods | GPL-3.0 (stated in README) | Kotlin Xposed module, HyperOS | System-level QCY control |
| https://github.com/hui1601/QuickyAndroid | none | Kotlin | Android port of Quicky |

None of the QCY projects above mentions JieLi or RCSP. They all use the 0xA001 service, so **no public project yet handles QCY's JieLi-based models**.

---

## 6. QCY's own protocol (service 0xA001) and the control-panel API

### 6.1 Protocol

Source: Quicky `docs/protocol.md` and `docs/service.md`. Reliability **B** for HT07; other projects confirm it on MeloBuds Pro, N70 and HT05.

- **Discovery.** BLE manufacturer data with company ID **0x521C**. 0x05D6 is used by QCY watches.
  - `data[0..1]` = vendorId (BE). This maps to the model.
  - `data[5..7]` = battery for L / R / case (bit 7 = charging).
  - `data[11..16]` = control MAC in scrambled order `[12]:[11]:[13]:[16]:[15]:[14]`.
  - `data[18..23]` = the other bud's MAC.
- **GATT.** Service `0000a001-…`.
  - Command write is `00001001`; notify/read is `00001002`.
  - Direct characteristics:
    - `0007` version
    - `0008` battery
    - `000B` EQ, written raw
    - `000D` key functions V2, written raw: pairs `[key, func]`
    - `000E` ZR-chip device settings
    - `000F` in-ear state (JL devices)
    - `0001`–`0006` V1 per-gesture keys
- **Frame.** `FF len {cmd n params…}…`
  - `len` is the total length minus 2.
  - Several command blocks can be packed into one frame.
  - `0xFE <cmd>` requests the current value of a command.
- **Command list:** 0x01 reset default, 0x02 clear pairing, 0x03 factory reset, 0x04 music control, 0x06 in-ear detection, 0x08 volume, 0x09 low latency, 0x0C noise mode, 0x10 sleep, 0x14 auto-off, 0x16 balance, 0x17 ANC scene `[mode, sub, value]`, 0x18 rename, 0x1D tone volume, 0x20 / 0x22 EQ params v1/v2, 0x23 LDAC, 0x2B key function, 0x2C wearing detection, 0x2D spatial audio, 0x2F battery, 0x30 version, 0x44 max EQ count, 0x46 / 0x47 per-channel EQ, 0x48 in-ear sensitivity, 0x4A game config, and others.
- **ANC scene values differ between models.** Quicky's table uses mode 2/3/4 for the silent/working/noisy scenes and 0x0A for transparency. MelobudsNext (MeloBuds Pro) uses mode 1 + sub 1..5, and mode 3 for transparency. **Take scene values from each model's control-panel JSON, not from a fixed table.**

### 6.2 Control-panel API

Source: Quicky `scripts/dump_products.py`. Reliability **B**.

- **Product list:** `POST https://api.watch.qcy.com/product/findProductList` with an empty body. The reply contains a URL to a ZIP of all products as JSON.
- **Control panel:** `POST https://api.watch.qcy.com/product/findControlPanels`, sent as a form.
  - Form fields: `modelId=<vendorId>` (the vendorId, *not* the flag ID) and `firmwareVersion=`.
  - Headers: `lang: en`, `country: US`, `sys_: android`, `app_version: 4.0.7_695`.
  - Reply: `data.controlPanel.layouts[]`.
- **Layout type codes:** 2 eq, 3 find_earphone, 4 settings, 8 channel_balance, 9 anc (`modes[]` with `startcmdid`, `endcmdid`, `defaultcmd`, `viewtype`, `items[]`), 10 key_function, 100 reset, 101 device_name, 102 auto_off_timer.
- **Settings item types:** 1 firmware_update (**ignore**), 3 reset_default, 100 factory_reset, 200 sleep_mode, 203 game_mode.
- A GitHub code search for `findControlPanels` / `api.watch.qcy.com` also returns this user's own public repo `hm-aktaruzzaman-mukdho/SonicTune` (`crates/qcy-protocol/src/profile.rs`, `docs/PROTOCOL.md`). That is useful to know: the project's protocol docs are already public.

---

## 7. Open questions for this project

1. **Does QCY use the default JieLi link key?** Grep `research/decompiled/` for `setLinkKey`, `06775f87` / `{6, 119, 95` or `0x06, 0x77`, and for `RcspAuth` / `getEncryptedAuthData` / `getRandomAuthData` in `com.jieli.jl_rcsp.impl`. A different 16-byte array there would be QCY's key. The E1 algorithm itself is fixed.
2. **Does QCY's JieLi firmware require auth at all?** Firmware can be built with `BT_CONNECTION_VERIFY` (auth skipped), and the app can turn it off with `setUseDeviceAuth`. Check the QCY app's `BluetoothOption` setup.
3. **Which attribute set do QCY JieLi models use** (ADV 0xC0 / 0xC1 or sys-info 13 / 14), and do they use the custom 0xFF opcode to tunnel QCY `FF len …` TLV frames? Look at how the QCY app calls `buildCustomCmd` / `sendRcspCommand`.
4. **What is the default SPP UUID** in the JieLi SDK? It is not found in public docs yet. It would be in `BluetoothConstant.UUID_SPP` in the decompiled app.

---

## Appendix: local copies

| Path | Content |
|---|---|
| `research/web_sources/jieli_fw_sdk/AC63/` | `JL_rcsp_packet.h`, `JL_rcsp_protocol.h`, `JL_rcsp_api.h`, `attr.h`, `rcsp_bluetooth.c` (Apache-2.0) |
| `research/web_sources/jieli_fw_sdk/AW31N/`, `AW30N_rcsp_auth.h` | later SDK versions of the same, Apache-2.0 |
| `research/web_sources/jieli_fw_sdk/JL701N_earphone/` | ADV module, ANC and key setting sources (unlicensed mirror; reference only) |
| `research/web_sources/jieli_ios_headers/` | `JL_OpCode.h`, `JL_RCSP.h`, `JL_TypeEnum.h`, `JLModel_ANC.h` |
| `research/web_sources/jieli_auth_js/` | JieLi's `jl_auth_2.0.0.js` / `.d.ts` (Apache-2.0) |
| `research/web_sources/jieli_home_sdk_docs_en/` | text dump of the JieLi Home SDK docs (no OTA pages) |
| `research/web_sources/jieli_home_assets/` | headset JSON configs |
| `research/web_sources/oss/` | auth implementations and write-ups from the projects above |
| `research/web_sources/oss/qcy_projects/` | READMEs of the QCY projects and the Quicky docs |
| `research/tools/bt_e1_check.py` | clean-room Bluetooth E1, verified against the device KATs |
| `research/tools/crawl_sphinx_docs.py` | crawler used for the docs dump (skips OTA/upgrade pages) |
