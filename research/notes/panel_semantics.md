# QCY control-panel JSON: format and semantics

Source: QCY Android app 4.0.7 (build 717), jadx 1.5.6 output in
`research/decompiled/sources/` (all Java paths below are relative to
`research/decompiled/sources/com/qcymall/` unless absolute). Where the
normal jadx output was mangled, single classes were re-decompiled with
`jadx -m simple` into `research/decompiled_simple/` (line numbers marked
"simple:" refer to those files).

Panels: 193 files in `research/panel_json/models/` (186 with a panel, 7 with
`data: null`), plus `research/panel_json/melobuds_pro_19786.json`.

Scope: everything a data-driven desktop UI needs. **Firmware update (item
type 1, `ota/` packages, service 0x7033) is out of scope and is only named so
it can be skipped.** JieLi (`isJLDevice`) and ZR (`isZRDevice`) branches are
named but not described; they do not use the TLV protocol on 0x1001.

Conventions:

* "TLV" = one entry `{id, n, payload[n]}`; a frame is `FF len TLV…` written to
  characteristic `0x1001` (service `0xA001`), `len` = bytes after itself
  (`qcylibrary/dataBean/DataAnalyse.java:9-21`).
* "read X" = write `FF 03 FE 01 X` (`DataBean.getRequestDataCMD`,
  `DataBean.java:349-351`); the answer arrives as a TLV in a frame notified on
  `0x1002` and is parsed by `DataBean.getDataBean` (`DataBean.java:101-328`).
* `optInt`/`optBoolean` = Android `org.json` accessors, which coerce strings
  and floats: `"10"`, `10`, `10.0` all read as 10, `"1"`/`"true"` as true. The
  panels really contain such variants (19805 MeloBuds A30 has every number as
  a string; 19784/19787/19788/19789 have `103.0`/`29.0`; 19746 has item type
  `201.0`). **A desktop parser must coerce `type`, `cmdid`, `jump`, `id`, … the
  same way.**

---

## 0. How the app gets and consumes a panel

### 0.1 Fetch and fallback

`manager/controlpan/ControlpanelJSONManager.java:1378-1449`
(`requestNewPanelJSON2`):

1. If a cached file `<name>_<vendorId>` exists → parse it (`onJsonChange(2)`).
2. Else if `localJsonMap` has the vendor id → bundled raw resource. **The map
   is empty in 4.0.7** (anonymous `HashMap` subclass without entries,
   line 67; simple:476), so `res/raw/controlpan_t11.json` is unused.
3. Else bundled default: `res/raw/controlpan_jl.json` if `isJLDevice()`, else
   `res/raw/controlpan_default.json`.
4. Then (always, unless cached and `z == false`) `POST
   product/findControlPanels` with `modelId=<vendorId>&firmwareVersion=<ver>`
   (`v3ui/mamager/WatchHttpAPI.java:722-727`); the `data.controlPanel` object
   replaces the cache and is re-parsed.

The server response also carries `firmwareVersions`, `firmwareversionGte`,
`firmwareversionLte` next to `controlPanel` (all downloaded panels:
`0.0.0..999.999.999`); the app never reads them, the server selects the
panel by `firmwareVersion`.

Defaults (useful as fallbacks for models with `data: null`, e.g. 6577, 16737,
16823, 18812, 18814, 23875, 23885):

| raw resource | layouts |
|---|---|
| `controlpan_default.json` | 10 (keys), 2 (EQ), 4 [100 Reset to default] |
| `controlpan_jl.json` | 1 (old keys), 2, 4 [3 Reset to default, 100 Clean pairing records] |
| `controlpan_t11.json` (unused) | 10, 2, `items`-dict layout, 4 [203/c9, 1 OTA, 100, 100] |

### 0.2 `analyseJSON`: layout type → internal lists

`ControlpanelJSONManager.analyseJSON` (normal: lines 274-486, simple:
638-887). Layouts are visited **in array order**; the resulting lists keep
that order. `type` is read with `optInt` (missing/non-numeric → 0 → ignored).

| layout `type` | handler | goes to | viewType (`JSONKEY_TYPE="viewType"`) | added to `readCmdIndexList` |
|---|---|---|---|---|
| 1 | `analyseJSONType1` | `keyFuncJsonOld` | 1 | – |
| 2 | `analyseEQJSON` | `musicEQJson`, `customEQJson`, `sysEQNameMap` | (EQ tab) | – |
| 3 | `analyseFindJSON` | new `{name, viewType:8}` → otherSingle+otherSetting | 8 | – |
| 4 | `analyseOtherDetailJSON` | per item, see §2 | 3/5/6/7 | each item's `cmdid` |
| 6 | `analyseANCJson` | `ancJson`, `ancModelTitle`, `ancAlertMsg` | (EQ tab) | – |
| 7 | `analyseANCLavelJson` | `ancLavelJson` | (EQ tab) | – |
| 8 | inline | `hasBalanceModel=true`, `balanceModelTitle=name` | (EQ tab) | – (balance read separately) |
| 9 | `analyseANCSenceJson` | `ancSenceJson`, `levelNames`, toast tips | (EQ tab) | 23, and 50 if any mode has `envAdaptation` |
| 10 | `analyseJSONType10` | `keyFuncJson` (music.event), `localKeyFuncJson` (sdMode.event), `touchCheck` → otherDitall+otherSetting | 1 / 5 | – |
| 11 | `analyseANCLavelJsonPercent` | `ancPercentLavelJson` | (EQ tab) | – |
| 12 | `analyseANCSenceLavelJsonPercent` | `ancSencePercentLavelJson` | (EQ tab) | – |
| 14 | inline | `tKeyFuncJson` | 10 | – |
| 20 | inline | `hasVolumeModel=true` | – | – |
| 21 | `analyseVoiceTypeJson` | `voiceTypeJson` | – | – |
| 23 | `analyseGameEQJson` | `gameEQJson` | (EQ tab) | – |
| 101 | inline | `moreSingleJson` ("More" page) | 3 | – |
| 102, 103 | inline | otherSingle+otherSetting, `hasPowerManager` | 3 | `cmdid` if present |
| 104 | inline | otherSingle+otherSetting | 3 | `cmdid` if present |
| 105 | inline | otherSingle+otherSetting | 3 | 53, 54 |
| 106 | inline | `musicItemJson` (shown only in SD-mode, §5) | 3 | 57 |
| 107 | inline | `modeItemSelectJson` | – | `cmdid` |
| 108 | inline | `whiteMusicJson` | – | – |
| 109, 110, 112, 113, 114, 116, 117, 118 | inline | otherSingle+otherSetting | 3 | `cmdid` |
| 111 | inline | `musicModeItemSelectJson` | – | `cmdid` |
| 115 | inline | `aiAncJson` | – | `cmdid` |
| 1000 | inline | otherSingle+otherSetting; `subItem[]{key,name}` is folded into `itemName{key:name}` | 3 | `cmdid` if present |
| 1001 | inline | otherSingle+otherSetting **only if** `languages` is empty or contains the phone's ISO language (e.g. `"zh"`) | 3 | `cmdid` if present (even when hidden) |
| 13, 24, 0/absent, anything else | – | **ignored** | – | – |

Notes:

* Layout types 13 (`{"cmdIndexs":[29],"msg":"Instruction ID to be read"}`,
  4577/4578/4580/4581) and 24 (`{"cmdid":13,"funcIDs":"3,…,12","title":"Touch
  detection"}`, 19842) are not handled by 4.0.7 at all (no `case`, no other
  reader of `cmdIndexs`/`funcIDs`).
* Two panels (4579, 19782) contain a layout without `type` whose `items` is a
  JSON **object** keyed by index (`{"0":{"title":"Gaming mode"},"3":{"msg":…}}`).
  Android ignores it (type 0). It looks like an iOS title/message override
  table. Ignore.
* The `readCmdIndexList` is replayed after connect and after a reset: one
  `FF 03 FE 01 <id>` per entry, 20–300 ms apart
  (`v2ui/fragment/DeviceInfoFragment.java:560-571, 826-835`;
  `v2ui/view/SingleButtonItemView.java:252-261`).

### 0.3 Where things are rendered

* **Settings tab** (`v2ui/fragment/BTNFunctionInfoFragment.java:96-186`) builds
  one list, rendered by `adapter/ButtonFunctionRecyclerAdapter.java:84-191`
  keyed on `viewType`:
  1. `tKeyFuncJson` (type 14) → viewType 10 `MutiButtonFunctionView`
  2. if `localKeyFuncJson` non-empty: a viewType 11 `ModeSelectItemView`
     (BT mode vs SD-card mode). When the cached value of cmd 57 is 1, the SD
     key events are shown instead of the normal ones.
  3. header row (viewType 0, left/right bud images) + key events
     (`getKEyFuncList(checkSuport("000d"))`, viewType 1 `ButtonFunctionItemView`)
  4. `musicItemJson` (type 106) only in SD mode
  5. `otherSettingJson` in parse order (see table above). The parallel
     lists `otherSingleJson` (`getSingleItemList`) and `otherDitallJson`
     (`getDetailItemList`) are never read by the UI.
  6. a final "More" row (viewType 7, `type:-4`) that opens
     `MoreEarphoneSettingActivity` with `moreSingleJson` + an "Earphone info"
     row (`type:-2`). The More page renders **every** entry with
     `SingleButtonItemView` (`adapter/MoreEarphoneSettingAdapter.java:60-74`).
* **Sound tab** (`v2ui/fragment/EQInfoFragment.java`): ANC (6/7/9/11/12/115),
  EQ (2/23), balance (8), music mode (111), mode select (107).
* viewType → view class: 1 `ButtonFunctionItemView`, 2 `SwitchInfoItemView`,
  3 `SingleKVItemView`, 4 `ImageInfoItemView`, 5 `SingleInfoItemView`,
  6 `SingleSwitchItemView`, 7 `SingleButtonItemView`, 8 `FindEarbudsItemView`,
  9 `POPStyleItemView`, 10 `MutiButtonFunctionView`, 11 `ModeSelectItemView`
  (all in `v2ui/view/`).

### 0.4 How values come back (generic path)

`QCYHeadsetClient.progressData` (`qcylibrary/QCYHeadsetClient.java:837-907`)
splits a `0x1002` frame into TLVs (`DataAnalyse.analyseAllCMD`, which requires
`frame.len == byte[1] + 2`) and dispatches by parser class.
`DataBean.getDataBean` (`DataBean.java:101-328`):

| id | parser | callback / storage |
|---|---|---|
| 4, 31, 52, 53, 55 | `SingleDataBean` | `onSingleValueResult` |
| 6 | `RuerDataBean` (`[0]==1`) | `SINGLEVALUE_6` = 0/1 |
| 7 | `NoiseDataBean(0)` | `onNoiseValueChange` (ANC, §3) |
| 8 | `VolumeDataBean` `[L,R,max]` | volume |
| 9 | `DiyanshiDataBean` (`[0]==1`) | `SINGLEVALUE_9` = 0/1 |
| 12 | `NoiseDataBean(1)` (mode) | §3 |
| 13 | `TestModeDataBean` (`[0]`) | EventBus 54 |
| 16 | `SleepDataBean` (`[0]==1`) | `SINGLEVALUE_16` = 0/1 |
| 17 | `CompactnessDataBean` | fit test |
| 18 | `LEDModeDataBean` (`[0]==1`) | `SINGLEVALUE_18` = 0/1 |
| 20 | `PowerManagerDataBean` | §6 |
| 22 | `BalanceDataBean` (`[0]` 0..100) | balance |
| 23 | `ANCSettingDataBean` | §3 |
| 24 | `PairnameDataBean` | pair name |
| 29 | `ToneVolumeDataBean` (`[0]` level, `[1]` max or 100) | `SINGLEVALUE_29` = level |
| 32 / 34 | `EQParamListBean` | §4 |
| 41 | `SingleDataBean` (1-byte payload duplicated `[b,b]`) | |
| 43 | `KeyFuncDataBean` | §5 |
| 44 | `PeidaiDataBean` | wear detection |
| 54 | `LEDEffectDataBean` | LED |
| 58 / 59 / 63 / 77 / 79 / 81 | music state / music files / alarms / phones / phone status / sensor | |
| 84 | `AIANCSettingDataBean`; 85/86 `AIANCConfigDataBean` | §3 |
| 88 | `SingleDataBean`, value = `b0<<24 \| b1<<16 \| b3<<8 \| b2` | hearing health |
| **anything else** | payload ≤ 2 bytes → `SingleDataBean`; longer → `CmdHexDataBean` | |

`SingleDataBean.value` (`SingleDataBean.java:15-26`) = `b0` for one byte,
`b0 | b1<<8` (u16 LE) for two bytes, 0 for an empty payload.
`CmdHexDataBean.name` = lowercase hex of the payload (`ByteUtils.byte2Hex`,
`qcylibrary/utils/ByteUtils.java:20-29`).

`QCYConnectManager` listener (`manager/QCYConnectManager.java:2027-2131`)
caches them per BLE MAC:

* `"<mac>_SINGLEVALUE_<id>"` (int) ← `onSingleValueResult` (all except
  `id 55 value 255`), the bool parsers above (as 0/1), and app-side
  `putSingleKVValue` on user toggles (optimistic update).
* `"<mac>_STRINGVALUE_<id>"` (string) ← `CmdHexDataBean` hex, or `%02X` of a
  single value.

**Boolean semantics used by all generic switches:** "on" ⇔ cached value
`== 1` (`SingleSwitchItemView.java:129-131, 199, 225-233`). Writes use the
panel's bytes, which by convention are `01` on / `02` or `00` off (§2.2).

---
### 0.5 Generic read/write helpers (non-JieLi, non-ZR path)

All in `manager/QCYConnectManager.java`; every one first checks
`isBleConnected()` and routes JieLi devices to `JLDeviceImpl` instead.

| helper | lines | bytes on `0x1001` (or other characteristic) |
|---|---|---|
| `requestCMDData(mac, id)` | 1276-1289 | `FF 03 FE 01 id` |
| `setSingleValue(mac, id, v)` | 1204-1217 → `QCYHeadsetClient.setSingleValue` (455-457) | `FF 03 id 01 v` |
| `setBytesValue(mac, id, b[])` | 1219-1236 → `QCYHeadsetClient.setBytesValue` (459-465) | `FF (n+2) id n b…` |
| `sendCMDData(mac, id, b[])` | 1320-1333 | same as `setBytesValue` |
| `sendBleData(mac, uuid, raw)` | 1335-1375 → `QCYHeadsetClient.sendBleData` (762-772) | **raw bytes verbatim** to `uuid` (used by all panel `cmd` strings). JieLi: maps 0x000F→in-ear, 0x000A `01/02/03`→restore/clear-pair/factory-reset via RCSP |
| `requestBalanceValue(mac)` | 1249-1274 | `FF 03 FE 01 16` (read 22) |
| `setBalanceValue(mac, v)` | 1133-1157 → `BalanceDataBean.getBalanceCMD` | `FF 03 16 01 v` (v 0..100) |
| `setPowerManager(mac, min)` | 1174-1187 → `PowerManagerDataBean.getPowerCMD` | `FF 06 14 04 lo hi 00 00` |
| `setPeidaiConfig(mac, en, music, anc)` | 1189-1202 | `FF 05 2C 03 en music anc` |
| `setLEDEffect(…)` | 1159-1172 | `setBytesValue(54, LEDEffectDataBean.getLEDEffectValueCMD(…))` |
| `setPairname(mac, s)` | 1108-1131 → `PairnameDataBean.getNameCmd` | `FF len 18 n utf8…` |
| `setCompactnessEnable(mac, on)` | 1377-1390 | `FF 05 11 03 e e e` (e = 1/0) |
| `readSettingStatus(mac)` | 854-897 | **read characteristic `0x000F`** (byte0 = in-ear (id 6) unless `0xFF`, byte1 = monitor (id 10) unless `0xFF`; stored under `"<mac>_BOOL_<id>"`, EventBus 51/35) **and read `0x1002`** (full settings dump → `progressData`). ZR: reads `0x000E`. JieLi: nothing |
| `readLanuage(mac)` | 1071-1106 | read characteristic `0x0009` as ASCII, split on `,`, first element = current prompt language (legacy V1 language) |
| `readEQData` / `readKeyFunction` | 813-852 / 899-1029 | see §4 / §5 |
| `setNoiseMode(mac, packed)` | 1462-1483 → `QCYHeadsetClient.setNoiseMode` (417-430) | see §3 |
| `setNoiseLevelPercent(…)` | 1392-1456 | `FF 05 17 03 mode scene level`, then read 23 if mode changed |

`QCYHeadsetClient.sendBleData` silently drops the write if the target UUID was
not discovered (`deviceSuportCharacter`, lines 766-769); `readBleData` likewise
(748-751). `sendBleCmdData` (774-787) is the same for 0x1001 writes made by
the library. **A desktop app should likewise skip panel entries whose
`character` is not present on the device.**

Read-back after connect (`DeviceInfoFragment.java:793-841`), 60 ms apart:
`readEQData`, `readKeyFunction`, `readSettingStatus` (0x000F + 0x1002 dump),
`readVersion` (0x0007), `readLanuage` (0x0009), then every id in
`readCmdIndexList` (`FF 03 FE 01 id`, 20 ms apart), then `requestBalanceValue`
if a type-8 layout exists.

---

## 1. Layout types found in the downloaded panels

Counts over the 186 panels (strings/floats coerced). "Chips" is the `chip`
field written by the downloader.

| type | panels | chips | meaning | desktop |
|---|---|---|---|---|
| 1 | 108 | JieLi 79, Generic 20, ZR 8, WuQi 1 | legacy key table (per-gesture characteristics), §5.3 | yes (non-JL: §5.3) |
| 2 | 185 | all | EQ, §4 | yes |
| 3 | 180 | all | "Find headset" (map + play sound on phone) | optional, §7.1 |
| 4 | 186 | all | settings item list, §2 | yes |
| 6 | 7 | Realtek 5, Generic 2 | legacy ANC mode list, §3.2 | yes |
| 8 | 101 | Generic, JieLi, WuQi | L/R balance, §7.2 | yes |
| 9 | 51 | JieLi 27, WuQi 20, Generic 4 | ANC modes with scenes/levels, §3.1 | yes |
| 10 | 52 | Generic, Realtek, WuQi, ZR 1 | key functions (event list), §5.1 | yes |
| 11 | 4 | JieLi 4 | ANC mode + percent level, §3.4 | JieLi only in practice |
| 13 | 4 | Generic | "cmdIndexs" (ids to read) – **ignored by 4.0.7** | read the listed ids anyway (harmless) |
| 14 | 27 | Generic 13, JieLi 13, WuQi 1 | headphone button table (keyName/events), §5.2 | yes |
| 20 | 3 | Generic | "Volume" model (`display`,`title`) → `hasVolumeModel`, §7.3 | yes (cmd 8) |
| 23 | 1 | Generic (19489 G1) | game EQ presets, §4.6 | yes |
| 24 | 1 | Generic (19842 T8S) | touch detection test (cmd 13) – **ignored by 4.0.7** | skip |
| 101 | 102 | | rename (Bluetooth name), §6.1 | yes |
| 102 | 102 | | timed power-off (sleep timer, cmd 20), §6.2 | yes |
| 103 | 70 | | prompt-tone volume (cmd 29), §6.3 | yes |
| 104 | 11 | WuQi | wear detection (cmd 44, + sensitivity cmd 72), §6.4 | yes |
| 106 | 1 | WuQi (19791) | local/SD music player (cmd 57) | no (§7) |
| 110 | 20 | | fit test (cmd 17), §6.5 | possible |
| 111 | 2 | JieLi (16764/16765) | music/game/movie mode (cmd 46), §6.6 | JieLi only |
| 112 | 10 | JieLi 9, Generic 1 | game mode config (cmd 74), §6.7 | yes (simple) |
| 113 | 6 | JieLi 5, Generic 1 | multipoint + paired phones (cmd 36/77/79), §6.8 | partial |
| 114 | 2 | JieLi | head gestures (cmd 80/81) | JieLi only |
| 115 | 5 | JieLi 4, Generic 1 | AI ANC (cmd 84/85/86), §3.6 | later |
| 116 | 2 | JieLi | spatial audio mode (cmd 91) | JieLi only |
| 1000 | 71 | | generic "pick one of N hex values" (cmd 31/46/51), §6.9 | yes |
| 1001 | 42 | | prompt language (cmd 25), §6.10 | yes |
| (none) | 2 | | `items` object (iOS overrides) – ignored | ignore |

Parser-supported but absent from all downloaded panels: 7, 12, 21, 105
(LED effects), 107, 108 (white noise), 109 (alarm), 117, 118 (hearing
health).

---

## 2. Layout type 4: settings items

### 2.1 Placement (`analyseOtherDetailJSON`, simple:1228-1374)

For each object in `items` (non-objects skipped):

1. `name` := `title` if `name` is absent.
2. If it has `cmdid`: if `cmdid == 36` **and** 36 is already in
   `readCmdIndexList` (because a layout 113 came earlier) the item is
   **dropped**; otherwise `cmdid` is appended to `readCmdIndexList`.
3. Dispatch on item `type` (coerced int):

| item type | placement | viewType / view | `cmdid` defaulting |
|---|---|---|---|
| 300..399 | otherSingle + otherSetting, `discribeMSG` := `msg` | 5 `SingleInfoItemView` | – |
| 200..299 | **inserted at index 0** of otherSingle, appended to otherSetting | 6 `SingleSwitchItemView` | only if no `cmdid`: 200 → 6 if `character`==`0000000F…` and 2 sub_items and `sub_items[0].cmd` starts with `"01"`, or 10 if it ends with `"01"`; 201 → 6; 202 → 10; 203 → 16 |
| 1 | otherSingle + otherSetting; `otaType = manufacturers` | 3 `SingleKVItemView` → OTA activity | **OTA: skip** |
| 100 | `jump==1` → More page, else otherSingle+otherSetting | 7 `SingleButtonItemView` | `type` rewritten: → 3 if (`character`=`0000000A…` and `cmd`=`"01"`) or (`character`=`00001001…` and `cmd`=`FF020100`); → 4 if (`0000000A…`, `"02"`) or (`00001001…`, `FF020200`/`FF020300`); otherwise stays 100 |
| anything else (2, 3, 4, …) | as 100 (`jump==1` → More page) | 7 `SingleButtonItemView` | – |

Note the 203 default: a 203 item **without** `cmdid` gets 16 (sleep), but
every 203 in the panels carries an explicit `cmdid` (6, 9, 42, 64).

### 2.2 Switch items (types 200–206): `SingleSwitchItemView`

`v2ui/view/SingleSwitchItemView.java`. Fields: `title`, `msg` (sub-text),
`alert` (confirmation dialog text), `cmdid`, `character`,
`sub_items[] {cmd: hex string, boolVal: bool}`, `jump` ("0").

* **Display:** checked ⇔ `SINGLEVALUE_<cmdid> == 1` (lines 125-131, 196-200);
  refreshed by EventBus 35 (bool parsers) and 106 (single values) for the
  same `cmdid` (203-236). Ignores echoes for 2 s after a click on JieLi
  (`HONEY_CMD_TIMEOUT`).
* **Click** (lines 72-121):
  * `cmdid 39` (dynamic EQ) is refused unless the current EQ bean has
    `eqType == 1` (toast "txt_def_eq_use").
  * if `(vendorId & 0xF0) == 0x50` and not both buds connected
    (`isTWSConnect()`), `cmdid 45` (spatial) and `35` (LDAC) are refused.
  * non-empty `alert` → confirm dialog first.
* **Write** (`switchChange`, 139-176): cache `SINGLEVALUE_<cmdid>` = 1/0
  optimistically; JieLi + type 204/201/206 → RCSP sleep/in-ear/game API;
  otherwise pick the `sub_items` entry whose `boolVal` equals the new state and
  write `ByteUtils.stringToBytes(cmd)` **verbatim** to `character`
  (`sendBleData`), or for JieLi `sendCustomCMD(bytes)`.

Item-type meanings (type only matters for defaults/JieLi; non-JieLi devices
just send the bytes):

| item type | meaning (title) | JieLi special |
|---|---|---|
| 200 | generic switch | – |
| 201 | in-ear detection (V1 char `0x000F`, `01`/`02`) | `setJLInearMode` |
| 202 | monitor/transparency V1 (cmdid 10) – not in panels | – |
| 203 | generic switch (gaming 9, in-ear 6, anti-wind 42, adaptive volume 64) | – |
| 204 | sleep mode (cmdid 16) | `setJLSleepMode` |
| 206 | gaming mode (cmdid 9) | `setJLGameMode` |

Observed switch variants (all panels; "raw" = no `FF len` prefix):

| cmdid | title | on / off `cmd` | framing | chips |
|---|---|---|---|---|
| 6 | In-ear sensor | `FF03060101` / `FF03060102` | framed | WuQi, Realtek, Generic |
| (none, type 200/201) | In-ear sensor/detection | `01` / `02` to `0000000F…` | V1 char | JieLi 13, ZR 2, Generic 2 |
| 9 | Gaming mode | `FF03090101` / `FF03090102` | framed | Generic, Realtek, WuQi |
| 9 (type 206) | Gaming mode | **`FF03100101` / `FF03100102`** | framed | JieLi 54, **WuQi 1 (19782 HT07)** |
| 9 (type 200) | Gaming mode | `090101` / `090100` | raw | JieLi (32890) |
| 16 | Sleeping Mode | `FF03100101` / `FF03100102` | framed | all |
| 18 | LED | `FF03120101` / `FF03120102` (+`alert`) | framed | Generic (19489) |
| 27 | APTX (reboots, `alert`) | `FF031B0101` / `FF031B0102` | framed | Generic (18961, 19220) |
| 35 | LDAC Switch | `FF03230101` / `FF03230102` (19805: `…0100`) | framed | WuQi, Generic |
| 35 | LDAC Switch | `230101` / `230100` | raw | JieLi |
| 36 | Dual Device Connection | `FF03240101` / `FF03240102` | framed | WuQi, Generic |
| 36 | Dual Device Connection | `240101` / `240100` | raw | JieLi |
| 39 | Dynamic EQ | `FF03270101` / `FF03270100`; raw `270101/270100` | both | Generic / JieLi |
| 42 | Anti-wind noise | `FF032A0101` / `FF032A0100`; raw `2A0101/2A0100` | both | WuQi / JieLi |
| 45 | Spatial sound effects | `FF032D0101` / `FF032D0100` | framed | Generic, WuQi |
| 64 | AdaptiveVolume | `FF03400101` / `FF03400100`; raw `400101/400100` | both | Generic / JieLi |
| 83 | Eco Mode (ANC off at ≤20 % battery) | `FF03530101` / `FF03530100`; raw `530101/530100` | both | Generic / JieLi |
| 87 | Bass Enhancement | `570101` / `570100` | raw | JieLi |

Rules for a desktop implementation:

1. The panel bytes are what the app sends. Raw (unframed) strings occur
   **only** on JieLi panels (they go through RCSP `sendCustomCMD`); never write
   them to 0x1001 as-is. If needed, frame them as `FF len <raw>`.
2. The "off" byte is `02` for the old ids (6, 9, 16, 18, 27, 35, 36) and `00`
   for newer ids (39, 42, 45, 64, 83, 87) — copy the panel, don't guess.
3. **Panel bug:** type 206 "Gaming mode" carries sleep-mode bytes
   (`FF0310…`). On JieLi this is harmless (206 → RCSP game-mode API). On the
   one non-JieLi panel with it (19782, WuQi HT07) the official app really
   sends `FF 03 10 01 01` = **sleep mode on** when the user enables "gaming
   mode". Recommendation: for 206 build the frame from `cmdid` 9
   (`FF 03 09 01 01/02`) and treat the panel bytes as wrong.
4. State: read `cmdid` (it is in `readCmdIndexList`) and test `value == 1`.
   For in-ear V1 (char `0x000F`) the state is byte 0 of a read of `0x000F`.
   Ambiguity: the app stores that under `"<mac>_BOOL_6"`, not
   `SINGLEVALUE_6`, so the official switch for V1 in-ear probably never shows
   the device state (only the optimistic local value).
5. A reply with an empty payload yields value 0 (= off).

### 2.3 Button items (types 100, 3, 4, …): `SingleButtonItemView`

`v2ui/view/SingleButtonItemView.java:88-262`. Fields: `title`/`name`, `msg`
(dialog text), `character` (default `00001001…` if empty), `cmd` (hex string),
`jump` (1 → More page).

Click → if TWS and either battery is 0 or > 100 → toast "both earbuds
required"; else confirm dialog (`title`, `msg`); OK →
`sendBleData(character, stringToBytes(cmd))`, then:

* item type 4 → EventBus 22 `EVENT_CLEAR_PAIRED`;
* resets the "sound market" EQ (HTTP call) unless it was a clear-pairing;
* sleeps 2.5 s, then `readLanuage`, read 20, `readSettingStatus`, 0.5 s,
  `resetAllOldStatus`, `readEQData`, `readKeyFunction`, and replays
  `readCmdIndexList` (300 ms apart).

QCC (`(vendorId & 0xF0) == 0x10`) devices not in state 3 get a "single
earbud" dialog instead.

Observed button variants:

| item type | title | character | cmd | effect |
|---|---|---|---|---|
| 3 / 100→3 | Reset to default | `00001001…` | `FF020100` | TLV id 1 (`RESET_DEFAULT`), empty |
| 3 | Reset to default | `0000000A…` (V1 reset char) | `01` | V1 restore defaults |
| 3 | Reset to default | `0000000E…` (ZR) | `01` | ZR (skip) |
| 3, 4, 100→4 | Delete/Clear pairing records | `00001001…` | `FF020200` | id 2 (`CLEAR_PAIR`) |
| 100→4 | Delete pairing records | `0000000A…` | `02` | V1 clear pairing |
| 100 | Delete pairing records | `0000000E…` | `02` | ZR (skip) |
| 100→4 | Reset to factory | `00001001…` | `FF020300` | id 3 (`FACTORY_RESET`) |
| 100 | Reset to factory | `0000000A…` | `03` | V1 factory reset |
| 2 | Headphone voice language (19745 T5) | `00000009…` | none; `sub_items[{name,cmd}]` (`01` Mandarin … `08` Spanish) | **broken in 4.0.7**: rendered as a button whose `cmd` is empty, so an empty write is sent. Intended: write the chosen 1-byte code to `0x0009` (V1 language) |

Desktop: implement as "confirm, write bytes, wait ≈2.5 s, re-read
everything". Clearing pairing / factory reset disconnects the phone link.

### 2.4 Info items (300–399): `SingleInfoItemView`

Only 301 appears (19921 HT03S, `{"title":"Fit detection","type":301}`): a row
that opens `CompactnessActivityEarphone` (fit test, same as layout 110,
§6.5) (`SingleInfoItemView.java:54-67`; also `SingleButtonItemView.java:128-132`).

### 2.5 Item type 1

`{"manufacturers": n, "title": "Firmware update", "type": 1}` → OTA. **Skip.**

---
## 3. Noise control layouts

### 3.0 The packed value and the two commands

All ANC UIs end in `QCYConnectManager.setNoiseMode(mac, v)`
(`manager/QCYConnectManager.java:1462-1483`, rate limited: ignored unless
> 500 ms since the last ANC write, 2000 ms for vendor 16758) →
`QCYHeadsetClient.setNoiseMode` (`qcylibrary/QCYHeadsetClient.java:417-430`):

* `v <= 255` → **legacy cmd 12 `NOISEMODE`**: `FF 03 0C 01 v`.
* `v > 255` → `mode = v>>16 & 0xFF`, `scene = v>>8 & 0xFF`,
  `level = v & 0xFF`; **special case `(3,1,0)` is sent as `(3,2,0)`**; →
  **cmd 23 `ANCSETTING`**: `FF 05 17 03 mode scene level`
  (`ANCSettingDataBean.getANCSettingCMD`).

So the JSON decides which command is used purely by the magnitude of its
numbers: type 6 `cmdid` values (1..4) → cmd 12; type 9 `startcmdid`/
`defaultcmd`/… (≥ 0x010000) → cmd 23.

Read-back:

* cmd 23 (`ANCSettingDataBean`, needs ≥ 3 bytes `[mode, scene, level]`,
  `DataBean`→`QCYHeadsetClient.java:844-846`) →
  `Devicebind.setCurrentANCMode(mode, scene, level)`; the UI compares the
  packed `getCurrentANCModeIndex()` (`model/Devicebind.java:327-339`), which
  maps `(3,2,x)` back to `0x030100` (voice focus) and otherwise packs
  `mode<<16|scene<<8|level`.
* cmd 12 (`NoiseDataBean(1)`, `[mode]`) and cmd 7 (`NoiseDataBean(0)`,
  `[value]`) → `onNoiseValueChange(mode, value)` →
  `setCurrentANCMode(mode)` (≤ 255 → scene/level = -1, so
  `getCurrentANCModeIndex()` returns the bare mode). Both ids share one static
  `NoiseDataBean` (`DataBean.java:150-158, 213-219`): mode comes from the last
  cmd 12, value from the last cmd 7.
* **cmd 7 `NOISE` (noise strength) is never written by 4.0.7**:
  `QCYHeadsetClient.setNoiseValue`/`readNoiseValue` (413-415, 360-362, read =
  `FF 03 07 01 FF`) have no callers. Only its notification is parsed.
* Type 9 adds 23 to `readCmdIndexList`; type 6 adds nothing — legacy ANC
  devices report cmd 12 in the `0x1002` dump read by `readSettingStatus`.
* A mode change is applied by the earbuds after ~0.5 s; the app re-reads
  23 after `setNoiseLevelPercent` when the mode changed
  (`QCYConnectManager.java:1451-1454`). Recommend: write, then read 23 after
  ~1 s (this project already does that, `docs/PROTOCOL.md` §6).
* Gate used by the app before any ANC write from the scene pager:
  `checkCanTWSANCSet()` (`QCYConnectManager.java:157-163`): both buds must
  report 1..100 % battery unless `(vendorId & 0x0F00) == 0x0100`.

### 3.1 Layout 9: scene ANC (`ancSenceJson`)

Keys seen (51 panels): layout `name`, `info`, `modes[]`, optional
`toastTip`, `toastTip2`, `toastMsg` (wear-detection toasts,
`manager/ToastManager.java:250-260`), parser also reads `levelNames`
(custom-level labels), `isNoShow` (hide the ANC entry on the device card),
`alertMSG` — none of these three occur in the downloaded panels.

`modes[]` keys: `name`, `img`, `imgcheck`, `img_black`, `img_white`,
`startcmdid`, `endcmdid`, `defaultcmd` (all 146 modes), `viewtype` (all),
`items` (40), `voicecmdid` (19), `envAdaptation` (string array) + `envName`
(11), `hidePop` (5), `ancInvert` (6). Parsed by
`model/controlpanel/ANCSenceSeted.java:40-65`; missing `defaultcmd` →
`startcmdid`; `ancInvert` = min ms between ANC writes for this mode, default
1500 (`getAncInvert`, 220-223); `touchDisable`, `valueDescribes`,
`progress`, `describe`, `noSpaceTip` also read.

`modes[].items[]` keys: `name`, `startcmdid`, `endcmdid`, `describe`, images,
optional `viewtype`, `defaultcmd`, `noSpaceTip` (5), `ancInvert` (10).

Example (19786 MeloBuds Pro; identical in 23872 N70 except ANC
`defaultcmd` 66816):

```json
{"name":"Noise canceling","viewtype":1,"startcmdid":65792,"endcmdid":131071,"defaultcmd":65794,
 "items":[{"name":"Adaptive Noise Cancellation","viewtype":3,"startcmdid":66816,"endcmdid":66816,"defaultcmd":66816},
          {"name":"Indoor","startcmdid":65792,"endcmdid":65794},
          {"name":"Commuting","startcmdid":66048,"endcmdid":66050},
          {"name":"Noisy","startcmdid":66304,"endcmdid":66306},
          {"name":"Anti-wind noise","startcmdid":66560,"endcmdid":66560}]},
{"name":"Transparency","viewtype":2,"startcmdid":196864,"endcmdid":196870,"defaultcmd":196868,"voicecmdid":196864},
{"name":"Normal","viewtype":3,"startcmdid":131072,"endcmdid":131072,"defaultcmd":131072}
```
(65792 = 0x010100, 131071 = 0x01FFFF, 66816 = 0x010500, 196864 = 0x030100,
196870 = 0x030106, 196868 = 0x030104, 131072 = 0x020000.)

Semantics:

* **Selected mode** = the mode whose `[startcmdid, endcmdid]` contains the
  current packed value (`ANCSenceSeted.isTagSelected`, 183-186;
  `EQInfoFragment.checkAncSenceView`, `v2ui/fragment/EQInfoFragment.java:1586-1608`).
  The current value inside a mode's range is remembered as `lastCMD`
  (`setLastCMD`, ignored when outside the range).
* **Tapping a mode** (`view/itempage/AncSensePagerView.java:51-80`): sends
  `lastCMD` if one was seen in this session, else `defaultcmd`
  (`getLastCMD`, `ANCSenceSeted.java:101-104`). Exception: if that value is
  the adaptive scene (`v & 0xFFFF00 == 0x010500`) and some sub-item has a
  `noSpaceTip`, `defaultcmd` is sent instead. Example 19786 ANC first tap:
  `FF 05 17 03 01 01 02`.
* `viewtype` of a top-level mode (`EQInfoFragment.java:652-707`):
  * **1** – has sub-scenes: shows `items[]` as cards
    (`initSubSenseItemPager`, 893-919). Each card has a slider with
    `max = endcmdid − startcmdid`; tapping/sliding sends
    `startcmdid + slider` (`view/itempage/AncSubItemPagerItemView.java:110-200`;
    first tap uses the current slider position, 100 ms debounce).
    E.g. Commuting level 1 → `0x010201` → `FF 05 17 03 01 02 01`;
    Anti-wind → `FF 05 17 03 01 04 00`; Adaptive → `FF 05 17 03 01 05 00`.
    A sub-item with `noSpaceTip` shows that tip first if music mode (cmd 46)
    is 5/6 or spatial mode (cmd 91) is 2/3. If the mode has
    `envAdaptation`, read cmd 50 and show an "environment adaptation"
    switch: `FF 03 32 01 01/00` (`setSingleValue(50, …)`, 805-812); value
    shown with the `envAdaptation[]` strings.
  * **2** – level slider: `slider ∈ [0, endcmdid − startcmdid − 1]`, sends
    `startcmdid + 1 + slider` (737-771). For 19786 transparency: levels
    `0x030101..0x030106` → `FF 05 17 03 03 01 01..06`. If `voicecmdid ≠ 0`
    a "voice focus" checkbox is shown; checked sends `voicecmdid`
    (`0x030100` → rewritten to `FF 05 17 03 03 02 00`), unchecked re-sends the
    slider value (782-802). Vendors 19777/19778 (Melobuds ANC, HT05):
    slider positions ≥ 4 first show a warning ("level 4 or above may make
    the headset howl", string `tip_default_level4`) with the choice of level 4
    (`startcmdid + 4`) or the requested level (`bridgeModeTip`, 871-890).
  * **3** – plain button, nothing else shown (Normal/Off; also used for the
    "Adaptive" sub-item).
  * **4** – "custom" ruler: range `startcmdid & 0xFF … endcmdid & 0xFF`; a
    value `n` sends `(defaultcmd & 0xFFFF00) + n` (`sendCustomRulerValue`,
    853-869; throttled 2 s, 500 ms trailing). Labels from layout
    `levelNames[n]`. Seen in 3 panels (1 Generic, 2 JieLi).
  Counts: viewtype 1 = 40, 2 = 21, 3 = 82, 4 = 3 modes.

### 3.2 Layout 6: legacy ANC buttons (`ancJson`)

`{"name":"ANC","type":6,"modes":[{"name":"ANC","cmdid":1,img…},{"name":"Transparency","cmdid":3},{"name":"Normal","cmdid":2}]}`
(19393/19394/19396/19397/19649 Realtek HT02/HT03/HT01C, 19873/19921 Generic
HT01/HT03S; 19873 has 4 modes: 1 ANC, 2 Outdoor, 3 Environment, 4 Normal).

Rendered as image buttons (`EQInfoFragment.initAncModeView`, 1123-1156;
`TopicBean` type 2). Tap (`v2ui/view/eq/EQItemView.java:161-171`,
`EQInfoItemView.java:184`) → `setNoiseMode(cmdid)` → **`FF 03 0C 01
cmdid`**. The meaning of each number is only given by the panel's `name`
(`QCYHeadsetClient` constants: 1 `NOISEMODE_ANC`, 2 `NOISEMODE_HUWAI`
(outdoor), 3 `NOISEMODE_JIANTING` (monitor)). Selected = current mode from
cmd 12 equals `cmdid`.

### 3.3 Layout 7: ANC level (`ancLavelJson`) – not in any panel

`modes[] {name, img, imgcheck, startcmdid, endcmdid}` → `ANCLavelSeted`
(`EQInfoFragment.initAncModeLavelView`, 1086-1121). Tap sends
`setNoiseMode(index)` (initially `endcmdid`), then a seek bar 1..n sends
`startcmdid + progress − 1` (183-206). Command 12 or 23 depending on
magnitude (§3.0).

### 3.4 Layout 11: ANC + percent (`ancPercentLavelJson`) – JieLi only in practice

`modes[] {name, ancindex, progress, img…}` (18289/18290/18294/18295 JieLi;
`ancindex` 1 ANC, 3 transparency, 2 off; 18290 uses 65).
`v2ui/view/ItemViewANCPercent.java:242` → `setNoiseLevelPercent(ancindex,
percent)` → non-JieLi: `FF 05 17 03 ancindex (ancindex==2 ? 0 : 1) percent`
(`QCYConnectManager.java:1392-1460`). `progress` = max percent (0 → no
slider).

### 3.5 Layout 12: scene + percent (`ancSencePercentLavelJson`) – not in any panel

`ANCSencePercentSeted`; `ItemViewANCSencePercent.java:239` →
`setNoiseLevelPercent(lastCMD, percent)`; sub-scene cards send
`setNoiseLevelPercent(startcmdid, step*10)`
(`AncSubItemPagerItemView.java:202-204`).

### 3.6 Layout 115: AI ANC (cmd 84) – 5 panels (19805 + 4 JieLi)

```json
{"cmdid":84,"name":"AI Scene Noise Cancellation","type":115,"jump":1,
 "configs":[{"name":"Noise canceling","modelID":1,"sceneID":3,"levelID":2,"levelCount":0,
             "items":[{"name":"Indoor","modelID":1,"sceneID":1,"levelID":2,"levelCount":3,"describe":"…"},…]},
            {"name":"Transparency","modelID":3,"sceneID":1,"levelID":4,"levelCount":6},
            {"name":"Normal","modelID":2,"sceneID":0,"levelID":0,"levelCount":0}]}
```

`modelID/sceneID/levelID` = the cmd 23 triple used per AI scene. Read 84
(in `readCmdIndexList`) → `AIANCSettingDataBean`
(`qcylibrary/dataBean/AIANCSettingDataBean.java`): `[status, noiseScene,
mode, subScene, noiseValue, (touchIndex)]`, ≥ 5 bytes. Enable/disable:
`setAIANCEnable` (`QCYConnectManager.java:1543-1557`) →
`FF 08 54 06 (on ? 00 : 04) 00 00 00 00 01`. Per-scene configuration uses
cmd 85/86 (`setAIANCConfig`, 1591-1605: `FF 07 id 05 a b c d e`) and 90
preview (`setAIANCPreview`, `FF 06 id 04 a b c d`) from
`v2ui/activity/AIAncConfigActivity.java`; the byte meaning of those fields
was **not** fully traced (ambiguous). Recommendation: show the on/off state
only; leave configuration for later.

---
## 4. EQ layouts (type 2, type 23)

### 4.1 Layout 2 schema

Parsed by `ControlpanelJSONManager.analyseEQJSON` (normal 724-772) and,
per preset, `model/controlpanel/SysEQSeted(JSONObject)`
(`SysEQSeted.java:196-269`).

| key | seen | type | meaning / use |
|---|---|---|---|
| `name`, `info` | all | string | section title (`eqModelTitle`) / help HTML |
| `character` | all | UUID | **always `0000000B-…`** in every panel; not used to choose the write path |
| `count` | all | int | 10 (5 in 7 panels) – band count shown in the UI |
| `maxdb`, `mindb` | all | int | slider range ± (both are positive magnitudes, 6 or 8; `mindb` is read into `eqMinGain` but only `maxdb` is used by `v2ui/view/eq/CustomEQLayout`) |
| `basedb` | 94 | int | 0, or −5/−8/−9 on some graphic panels: **added to every gain of a custom (id & 0x80) graphic EQ before writing 0x000B** (`QCYConnectManager.java:641-649`) |
| `freq` | all | comma string | band centre frequencies. Parsed with `Integer.parseInt`, unparsable entries silently dropped: graphic panels use `"1k,2k,…"` (only 31,62,125,250,500 survive, irrelevant because no `q`); 7 panels (`"62,250,1K,4K,8K"`, 18863/19730/19732… and ZR) **lose 1K/4K/8K**, so the app sends only 2 bands for them (app bug). |
| `q` | 92 | comma string | per-band Q. **Presence of a parsable `q` (and `freq`) = parametric EQ** (`SysEQSeted.isSupportMultiEQ`, 96-100) |
| `iitType` | 67 | comma string | per-band filter type (0 = peaking; presets use 4 on the lowest band, probably a low shelf – not verified). **Presence = 7-byte bands (cmd 34)**, absence = 6-byte bands (cmd 32) |
| `eqs` | 85 | comma string | default gains of the custom curve |
| `volumelevel` | 85 | number | limit used by the auto master-gain (`getMaxGain`, `QCYConnectManager.java:387-421`; also `docs/PROTOCOL.md` §7) |
| `sys_eq[]` | all | array | presets, below |
| `aux_eq[]` | 0 | array | presets shown only when cmd 55 (play mode) == 4 (`getMusicEqJsonList(4)`, normal 572-591) |
| `individuation_eq` | 7 | object | one extra preset (personalised/hearing-test EQ), added to the preset list |
| `rebootTip` | 10 | string | dialog shown when switching **away from** preset id 10 or 18 (`EQItemView.java:387-402`) |
| `supportMaxCount` | 5 | int | **not read** by 4.0.7 |
| `modes` | 4 | | **not read** |

`sys_eq[]` entry keys: `name`, `eqs` (gains, dB, may be fractional),
optional `id` (425 of 1172; otherwise the app assigns `index + 1`),
`q`, `freq`, `iitType`, `mastergain` (dB), `img`, `checkimg`,
`alarmMsg` (confirmation text), `onlyTws` (refuse unless both buds), `vals`.
Presets carry `q`/`freq` exactly when the layout does, and `iitType` exactly
when the layout does (verified over all panels), so a panel is internally
consistent.

Three families (counts of EQ layouts):

| family | condition | panels | write | read |
|---|---|---|---|---|
| graphic | no `q` | 93 (Generic, JieLi, Realtek, WuQi, ZR) | char `0x000B`: `[id, g0..g9]` | read `0x000B` |
| parametric v1 | `q`, no `iitType` | 25 | cmd 32 `MULTIEQ` (6-byte bands) | read 32 and 34 |
| parametric v2 | `q` + `iitType` | 67 | cmd 34 `MULTIEQ2` (7-byte bands) | read 32 and 34 |

(JieLi/ZR use their own transports for all three.)

### 4.2 Selecting a preset (`EQItemView.eqItemClick`, `v2ui/view/eq/EQItemView.java:254-428`)

Gates: QCC device not fully connected → dialog; QCC in game mode → toast;
`isHQ3710()` with ANC on → toast; `onlyTws` and one bud → toast.
Special preset ids:

* **9** → adaptive EQ: `FF 03 27 01 01` (cmd 39 = 1), spinner 2 s, then the
  preset is applied (`EQItemView.java:306-316`).
* **10 / 18** → if LDAC is on (cached cmd 35 > 0) and the preset has
  `alarmMsg`, ask to disable LDAC first, then apply.
* **11** → separate left/right EQ (`setSideEQData`, cmd 70 left / 71 right,
  7-byte bands like cmd 34) using curves saved by `CustomEQEdit2Activity`.
* Others with `alarmMsg`: confirm (or toast if `alarmTime > 0`), then apply.

Apply = `QCYConnectManager.setEQData(mac, id, gainRealArray, preset)`
(`QCYConnectManager.java:423-666`):

**Graphic** (`!preset.isSupportMultiEQ()`, 624-657):
`gainRealArray[i] = (byte)(eqs[i]*10)/10` = integer dB (truncated).
Write to characteristic `0x000B` (`QCYHeadsetClient.setEQ`, 499-501,
`EQDataBean.getSetEqCMD`): `[id, g0, …, g9]` (signed bytes). If
`id & 0x80` (custom), each gain += `basedb` first.
Example 19393 HT03 preset 2 "Pop" (`-6,5,-3,-2,5,4,-4,-3,6,4`):
`0x000B ← 02 FA 05 FD FE 05 04 FC FD 06 04`.

**Parametric** (470-623): bands = preset `freq[i]`, `eqs[i]` (float dB),
`q[i]`, `iitType[i]` (255 if absent), `n = min(|freq|,|q|,|eqs|)`;
index = preset `id`; master gain = preset `mastergain`. Then padded with
`(f=0, g=0, q=0, type=255)` up to `min((MTU − 7) / 6, MAXEQ_COUNT)`
(MAXEQ_COUNT = cached cmd 68, default 20; JieLi 10). If more bands than
MAXEQ_COUNT, bands inherited from the previous custom curve are dropped
first (530-575).

* `iitType` present → cmd 34: `FF len 22 n idx mg_lo mg_hi {f_lo f_hi g_lo g_hi q_lo q_hi type}×k`
  (`QCYHeadsetClient.setEQWtihQFG2Type`, 515-574; g, q = round(x·100) i16/u16
  LE, master = trunc(mg·100) i16 LE).
* no `iitType` → cmd 32: same without the `type` byte
  (`setEQWtihQFG`, 576-619).
* Vendors 19779/19781/19782/19783 with a stored DB curve (129 ≤ dbSaveEQID
  < 240) re-send the saved hex payload with cmd 32, others with cmd 34
  (597-604) – this concerns curves saved in the app's database only.

Example 19786 preset 10 "Spatial sound effects" (`eqs 0,-8,-2,4,3,-3`,
`q 0.7,1,1.5,1,0.8,2`, `freq 30,220,900,2800,12000,5300`,
`iitType 4,0,0,0,0,0`, `mastergain 0`), MTU 220 → 20 bands:
`FF 91 22 8F 0A 00 00 | 1E 00 00 00 46 00 04 | DC 00 E0 FC 64 00 00 | 84 03 38 FF 96 00 00 | F0 0A 90 01 64 00 00 | E0 2E 2C 01 50 00 00 | B4 14 D4 FE C8 00 00 | (00 00 00 00 00 00 FF)×14`.

### 4.3 Custom curve

The custom editor (`v2ui/view/eq/CustomEQLayout.java:389, 708`) calls
`setEQData(mac, 255, gains, new SysEQSeted(layoutJson), changedFlags)`;
`CustomEQNewDialog` uses 128. Layout `id` is absent → `saveIndex 0`:

* graphic: `0x000B ← [FF, g0+basedb, …]` (or `80 …`).
* parametric: bands = the layout's `freq`/`q`/`iitType` with the slider
  gains (only changed bands when `changedFlags` given, plus the remaining
  bands of the last-sent curve), master gain = `getMaxGain` (auto, §7 of
  `docs/PROTOCOL.md`), **index = 255 the first time, 128 when a previous
  custom curve exists in the app DB** (`QCYConnectManager.java:518-521`).
  Ambiguity: whether the firmware distinguishes 128 and 255 is unknown; the
  MeloBuds Pro accepts 255.
* "EQ market" downloads (`setEQMarketData`, 730-811) use index **126**.

### 4.4 Reading the current EQ

`readEQData` (`QCYConnectManager.java:813-852`): parametric layout → read 32
and read 34 (`QCYHeadsetClient.readEQParams`, 330-333); graphic → read
characteristic `0x000B` → `EQDataBean` (`qcylibrary/dataBean/EQDataBean.java:86-101`):
`[eqType, g…]`; if exactly 13 bytes: `[eqType, g0..g9, gameEQType,
currentMode]` (currentMode 1 = music, 2 = game). Current preset = `eqType`;
`eqType & 0x80` = custom. cmd 68 is requested 1 s after every connect
(`QCYConnectManager.java:1739-1749`). cmd 32/34 replies → `EQParamListBean`
(index, master, bands; already implemented in `crates/qcy-protocol/src/eq.rs`).
The UI matches a reply to a preset by `index` (`saveIndex`), or for
`index & 0x80` by comparing the gain string (`EQInfoFragment.getEQName`,
1661-1675).

### 4.5 Other EQ-related switches

* cmd 39 (dynamic/adaptive EQ) switch exists as a type-200 item in 10 panels;
  the app only allows enabling it while the current EQ type is 1
  (`SingleSwitchItemView.java:81-85`).
* cmd 87 "Bass Enhancement" (JieLi panel only).

### 4.6 Layout 23: game EQ (19489 G1 only)

```json
{"type":23,"character":"0000000B-…","count":10,"freq":"31,…,16k","maxdb":8,"mindb":8,
 "sys_eq":[{"id":65,"name":"Default","eqs":"0,…"},{"id":66,"name":"Gunshots","eqs":"-6,5,-3,-2,5,4,-4,-3,6,4"},
           {"id":67,"name":"Footsteps"},{"id":68,"name":"Explosions"},{"id":69,"name":"Vehicle sounds"},{"id":70,"name":"Aircraft"}]}
```
`analyseGameEQJson` (normal 709-722; missing `id` → 0-based index). Shown
behind a Music/Game radio (`EQInfoFragment.initEQPager`, 556-600); a game
preset is applied exactly like a graphic preset: `0x000B ← [id, g0..g9]`
(e.g. Gunshots `42 FA 05 FD FE 05 04 FC FD 06 04`). The 13-byte `0x000B`
read reports the active game preset in byte 11 and the mode in byte 12.

Desktop: support all three families; ignore market/DB/L-R features.

---
## 5. Key (touch/button) function layouts

### 5.0 Common model: `EarphoneKey` and the write path

Every selectable key slot becomes an `EarphoneKey`
(`model/controlpanel/EarphoneKey.java:52-105`) built from an event object
`{eventID, character?, defaultFun?, default?, music?, list:[{name, cmd}]}`:

* **No `character`** (types 10 and 14; "V2") → `uuid = 0000000D-…`,
  `eventID = optInt("eventID")`, each list entry's `cmd` read with
  **`optInt` (decimal, strings coerced)**; bytes = `[eventID, cmd]` built via
  `"%02x%02x"` + `stringToBytes`. So `"cmd": 11` or `"cmd": "11"` → byte
  `0x0B`. (For eventID > 255 the hex string becomes odd-length and
  `stringToBytes` mangles it; only JieLi panels use such ids and they don't
  take this path.)
* **With `character`** (type 1; "V1") → `uuid = character`, `eventID` derived
  from the UUID (`0x0001`→1 … `0x0008`→8, `0x0009`→9, `0x000A`→10), list
  `cmd` is a **hex string** (`"0B"` = 11) and the bytes written are exactly
  `stringToBytes(cmd)` (one byte).

Picking a function (`ui/device/KeyFunctionSelectActivity.java:145, 217`) →
`QCYConnectManager.setKeyFunction(mac, uuid, bytes, eventID, cmd)`
(`manager/QCYConnectManager.java:1489-1541`):

* JieLi → RCSP per-gesture setters; ZR → own table on `0x000D`.
* otherwise: **if `cmd == 0` and `(vendorId & 0xF0)` ∈ {0x20, 0x30, 0x80}
  and firmware ≥ 1.1.0, the action byte is replaced by `0xFF`** (1534-1536);
  then `sendBleData(uuid, bytes)` (V2: `0x000D ← [eventID, action]`, a partial
  table is fine) followed by `readKeyFunction`.

Gate: the left/right picker only opens when that bud reports 1..100 %
battery (`v2ui/view/ButtonFunctionItemView.java:87-147`).

Read-back (`QCYConnectManager.readKeyFunction`, 899-963):

* JieLi → RCSP `updateSetting`.
* device has characteristic `0x000D` → read it → `KeyFuncDataBean`
  (`qcylibrary/dataBean/KeyFuncDataBean.java`): pairs `[eventID, action]`
  → map `"eventID" → action` (eventID taken as a **signed** byte); the same
  parser handles TLV id 43 (`KEYFUN`) notified on `0x1002`.
* otherwise (V1) read characteristics `0x0001`..`0x0006` one by one; byte 0
  = action of gesture 1..6. Long press (`0x0009`/`0x000A`) is never read.

Display: `EarphoneKey.setCurFunc(action)` selects the list entry whose
`cmd` equals the read action (`ButtonFunctionItemView.refreshKeyValue`,
230-242); no match → label unchanged (the panel's `default` string or
`defaultFun`). The app also caches the last selection per MAC in shared
preferences.

### 5.1 Layout 10 (52 panels): `music` / `sdMode` / `voice`

```json
{"type":10,"name":…,"info":…,"leftImg":url,"rightImg":url,
 "music":{"name":"Music mode","event":[
   {"name":"Touch","left":{"eventID":1,"default":"Play/pause","defaultFun":1,"list":[{"name":"Not work","cmd":0},{"name":"Play/pause","cmd":1},…]},
                   "right":{"eventID":2,…}},
   {"name":"Double touch","left":{"eventID":3,…},"right":{"eventID":4,…}}, …]},
 "voice":{"name":"Call mode","event":[{"name":"Double touch","left":"Answer/end call","right":"Answer/end call"}]},
 "sdMode":{…same shape as music…}, "sys":"android", "touchCheck":{…}}
```

* `music.event[]` → `keyFuncJson`; one row per event (`ButtonFunctionItemView`,
  left + right slot). eventIDs seen: 1/2 touch, 3/4 double, 5/6 triple,
  9/10 long press (7/8 quadruple exist in `QCYHeadsetClient.KEYID_*` but in
  no panel). `left`/`right` may be missing for one side.
* `voice` (30 panels) is **not parsed by 4.0.7** (only by the unused legacy
  `model/ControlerPanl.java:845`); its `left`/`right` are plain strings
  describing fixed call controls. `QCYHeadsetClient.KEYID_VOICE_*` (21..30)
  exist but are unused. Desktop: show as read-only text or ignore.
* `sdMode` (only 19791 Crossky R70, name "Focus Mode", eventIDs 81..84,
  89, 90) → `localKeyFuncJson`; a BT/Focus selector (`ModeSelectItemView`,
  `v2ui/view/ModeSelectItemView.java`) writes `FF 03 39 01 00` (BT) /
  `FF 03 39 01 01` (focus) (cmd 57 `FOCUS_MODE`); when cmd 57 == 1 the
  sdMode key rows replace the music rows (`BTNFunctionInfoFragment.java:111-146`).
* `sys` ("android", 18961/19220): not read.
* `touchCheck` (not in any panel): becomes a "touch test" info row
  (`TouchCheckActivity`).
* `leftImg`/`rightImg`: not read from the panel (bud images come from the
  product list).

### 5.2 Layout 14 (27 panels): headphone buttons (`tKeyFuncJson`)

```json
{"type":14,"name":…,"img":url,"buttons":[
  {"keyName":"Volume down","events":[{"eventID":17,"name":"Touch","list":[{"name":"Not work","cmd":0},…]}]},
  {"keyName":"Volume up","events":[{"eventID":18,…}]},
  {"keyName":"multifunctional","events":[{"eventID":19,"name":"Touch"},{"eventID":20,"name":"Double touch"},
                                         {"eventID":21,"name":"Triple touch"},{"eventID":22,"name":"Touch four times"}]}]}
```

Rendered by `MutiButtonFunctionView` (`v2ui/view/MutiButtonFunctionView.java:173-210`):
up to **3** buttons (tabs; extra buttons ignored), each event is a V2
`EarphoneKey` (side 3) → `0x000D ← [eventID, action]`.

eventIDs: non-JieLi panels use single-byte ids — 17/18 vol−/+ touch, 19..22
MFB 1..4 taps, 24/25/26/28 volume rocker touch/double/triple/long
(16849/16850 H4), 29..32 volume double/triple (4579/19780 Crossky Link2).
JieLi panels use `keyId<<8 | clicks` (769 = 0x0301, 1281 = 0x0501,
1793..1796 = 0x0701..0x0704) → `setBtnClickFunction(id>>8, id&0xFF, cmd)`
(RCSP, `QCYConnectManager.java:1515-1517`).

### 5.3 Layout 1 (108 panels, mostly JieLi): legacy per-gesture table

```json
{"type":1,"name":…,"info":…,
 "left":{"single_click":{"name":"Touch","character":"00000001-…","music":"Play/pause","list":[{"name":"Not work","cmd":"00"},{"name":"Play/pause","cmd":"01"},…]},
         "double_click":{…"00000003-…"},"three_click":{…"00000005-…"},"long_press":{…"00000009-…"}},
 "right":{"single_click":{…"00000002-…"},"double_click":{…"00000004-…"},"three_click":{…"00000006-…"},"long_press":{…"0000000A-…"}}}
```

`analyseJSONType1` (simple:1434-1557) turns each gesture into an event with
eventID 1/2 (single), 3/4 (double), 5/6 (three), 9/10 (long), `defaultFun`
= first byte of the list entry whose `name` equals `music`, `character`
copied → V1 `EarphoneKey`. Write = 1 byte to the gesture's characteristic.

Selection (`ControlpanelJSONManager.getKEyFuncList(z)`, normal 523-531,
called with `z = device has 0x000D`): if `z` and a type-1 list exists → type 1;
else type 10 list if non-empty; else type 1.

**Risk:** the V1 long-press characteristics are `0x0009` and `0x000A`, the
same UUIDs as the V1 prompt-language and V1 reset/clear-pairing
characteristics. Long-press entries only occur on JieLi panels (handled via
RCSP there). A desktop app must never write key bytes to `0x0009`/`0x000A`
on a non-JieLi device.

### 5.4 Action values (all panels)

| action byte | names in panels (count) | where |
|---|---|---|
| 0 | Not work / No function / N/A | all |
| 1 | Play/pause, Playback/pause | all |
| 2 | Previous track, Skip track backward | all |
| 3 | Next track, Skip track forward | all |
| 4 | Voice Assistant | all |
| 5 | Volume up (+) | all |
| 6 | Volume down (−) | all |
| 7 | Gaming mode, Low-latency (mode), Mode switching (21×, type 14) | all |
| 8 | Take pictures (`"08"`, type 1 only) | V1 |
| 10 (`"0A"`) | ANC mode / ANC (24×, type 1 only) | V1 |
| 11 (`"0B"` in V1, `11` in V2) | ANC mode / ANC / ANC switching / Transparency Mode | all |
| 14 | Focus Mode (type 10, 2×), bt/dongle (type 14, 1×) | |
| 15 | Spatial sound effects (type 10, 12×) | |
| 81..86 (0x51..0x56) | Default EQ, Gunshots, Footsteps, Explosions, Vehicle sounds, Aircraft sounds (19489 G1 – game-EQ presets 65..70 by key) | type 10 |
| 0xFF | written instead of 0 on vendors `(vid & 0xF0)` ∈ {0x20,0x30,0x80}, fw ≥ 1.1.0 | |

`QCYHeadsetClient` also names `FUNID_ANSWER 8`, `FUNID_REFUSE 9`,
`FUNID_HOLDON 10`, `FUNID_REDIAL 11` (call functions) — these conflict with
the panel meanings of 8/10/11 and are unused; trust the panel names.
Labels must come from the panel; the meaning of a byte is model specific
(e.g. 10 vs 11 for ANC; 14 = Focus vs dongle).

### 5.5 Layout 24 (19842 T8S) – not implemented in 4.0.7

`{"type":24,"cmdid":13,"funcIDs":"3,4,5,6,7,8,9,10,11,12","title":"Touch detection","info":"Please try touching the earphone touchpad."}`.
Probably: enable test mode (cmd 13 `TESTMODE`, `FF 03 0D 01 01/02`,
`TestModeDataBean`) and highlight the touched gesture (`EventBus 54` carries
the `TestModeDataBean` value). No handler exists in 4.0.7. Skip.

---
## 6. "Single KV" setting layouts (viewType 3, `SingleKVItemView`)

Common behaviour (`v2ui/view/SingleKVItemView.java`): row title = `title`
or `name`, sub-text = `desc`, icon from `icon` URL (types ≥ 107) or built-in;
value text computed in `getValueShow` (317-503) and refreshed on EventBus
105/106/107/108/109; a click opens a sub-page (101-243). All of these rows
require a BLE connection (else "connect?" dialog). `cmdid` is in
`readCmdIndexList` unless noted.

### 6.1 Layout 101: rename (102 panels; More page)

`{"type":101,"id":101,"jump":"1","hasValue":true,"msg":"Please upgrade the APP…","maxLength":20?}`
(`msg` is an "update the app" fallback text for old app versions; `hasValue`
unused). Opens `activity/EditPairdnameActivity.java`: UTF-8 name, max
`maxLength` bytes (default 32; 20 on JieLi H3 family) (158-166) →
`setPairname` → `FF (n+2) 18 n utf8…` (cmd 24). Refused when one bud is out
(`SingleButtonItemView.java:134-147`). Current name: read 24 →
`PairnameDataBean` (NUL padded). Desktop: supported.

### 6.2 Layout 102: timed power-off / sleep timer (cmd 20; 102 panels)

`{"type":102,"cmdid":20,"repeat":50000|65000,"msg":…,"itemName"?}` —
`repeat` and `itemName` are **not used** by 4.0.7.
`v2ui/activity/EarphonePowerManagerActivity.java`: fixed choices
`{65535 (off), 15, 30, 60, 90 min, custom}` (line 28); custom input 1..240
min (139-152); a separate "Shut down" button sends **0 = power off now**
(50-60). Write `setPowerManager(min)` → `FF 06 14 04 lo hi 00 00`. Read 20 →
`PowerManagerDataBean` `[set_lo, set_hi, left_lo, left_hi]` (≥ 4 bytes;
`left` forced to 1 if set and 0). Row text: "off" if set == 65535 or
left ≤ 0, else remaining "h:mm" / "m min" (`SingleKVItemView.java:351-379`).
Desktop: supported; **never send 0 to cancel** (use 0xFFFF).

### 6.3 Layout 103: prompt-tone volume (cmd 29; 70 panels)

`{"type":103,"cmdid":29,"itemName":{"4":"Lower volume","6":"Medium volume","8":"Higher volume","10":"Maximum volume"}}`
(19786). Keys are the **level values** (decimal strings); the set differs per
model (e.g. `3/6/8`, `6/8/13`, `2/4/8`, `3/4/7/9`, `16/3/8`).
`v2ui/activity/ToneVolumeManagerActivity.java:74-90`: vendor **16737
(0x4161)** → `FF 03 1D 01 v`; everyone else → `FF 04 1D 02 v 00`. Read 29 →
`ToneVolumeDataBean` `[level, max]` → `SINGLEVALUE_29 = level`; row shows
`itemName[str(level)]` (blank if no exact key). Desktop: supported.

### 6.4 Layout 104: wear detection (cmd 44; 11 WuQi panels)

```json
{"type":104,"cmdid":44,"subTitle":"Music Control","valueTitle":"ANC",
 "msg":"This setting will only take effect when you remove and then re-wear the earphones.",
 "ancMsg":"Noise cancelling off when taken off, noise cancelling on when put on.",
 "subItem":[{"cid":0,"name":"Close"},{"cid":1,"name":"Take off earbud to pause and put on to play"},
            {"cid":2,"name":"Take off earbud to pause but put on not to play"},{"cid":3,"name":"Take off earbuds to pause and put on both to play"}],
 "sensitivity":{"cmdID":72,"name":"Sensitivity","items":[{"cid":1,"name":"Medium"},{"cid":2,"name":"Normal"},{"cid":3,"name":"High"}]}}
```
`v2ui/activity/InearConfigActivity.java`: master switch + music radio
(`subItem[].cid`) + ANC switch → `setPeidaiConfig` → `FF 05 2C 03 en music
anc` (then read 44). Read 44 → `PeidaiDataBean` `[en, music, anc, (tone)]`.
Row text: "on" if `en == 1`. `sensitivity` (9 panels, N60/N70/E50/HT19…):
slider with `items.length` steps, writes `FF 03 48 01 <progress>` (cmd 72
`INEAR_SENSITIVITY`), read 72 (requested when the page opens, 92). Ambiguous:
the slider value is the step index (1..n); in all panels `cid` == index, so
sending `cid` is equivalent. Desktop: supported.

### 6.5 Layout 110 (and item 301): fit test (cmd 17; 20 panels)

`{"type":110,"cmdid":17,"jump":"1"}` → `CompactnessActivityEarphone`:
requires both buds; start `FF 05 11 03 01 01 01`, the phone plays
`res/raw/compactness` through the earbuds, the device notifies cmd 17
`[status, left, right]` (`CompactnessDataBean`); `status == 2` = result;
result 3 = good fit, 1/2 = poor fit, 0 = failed (strings
`txt_compactness_result0..3`); stop `FF 05 11 03 00 00 00`
(`QCYConnectManager.setCompactnessEnable`, 1377-1390;
`CompactnessActivityEarphone.java:156-262`). Desktop: possible (needs to play
a test sound to the A2DP sink); low priority.

### 6.6 Layouts 111 / 107 and 1000 with cmd 46: sound mode (music / game / movie)

111 (16764/16765 JieLi):
`{"type":111,"cmdid":46,"subItem":[{"index":1,"name":"Music mode"},{"index":3,"name":"Gaming mode"},{"index":4,"name":"Movie Mode"}]}`.
Rendered on the sound tab by `v2ui/view/ModeItemSelectView.java` (also for
107): tap → `setSingleValue(cmdid, index)` → `FF 03 2E 01 index`; selected
⇔ `SINGLEVALUE_46 == index` (300-349). The same setting appears in Generic
panels as layout 1000 (16818/16821 V200, keys `"01"/"03"/"04"`), §6.9. Music
mode values 5/6 are referenced as "spatial" modes by the ANC sub-scene
`noSpaceTip` check (`AncSubItemPagerItemView.java:129-131`).

### 6.7 Layout 112: game mode config (cmd 74; 10 panels, 1 non-JieLi: 19805)

`{"type":112,"cmdid":74,"confTip":"Enable and Sync Game Sound Effects","desc":"Switching Game Mode will reboot the headset"}`
→ `v2ui/activity/GameModeConfigActivity.java`: a switch + a "sync game EQ"
checkbox, written together as `FF 04 4A 02 <enabled> <sync>` (64, 79); read
74 (94). Row text: "on" if the hex string of the reply starts with `01`
(`SingleKVItemView.java:438-453`). Ambiguity: a 2-byte reply goes through
`SingleDataBean`, whose string form is `%02X` of the u16 (e.g. `[01 01]` →
`"101"`), so the official row text can be wrong; decode as
`[enabled, sync]`. Desktop: supported (warn about reboot).

### 6.8 Layout 113: multipoint / paired phones (cmd 36 + 77/79; 6 panels)

`{"type":113,"cmdid":36,"desc":"Connect up to 3 Bluetooth devices at once"}`
→ `v2ui/activity/PairPhoneConfigActivity.java`: switch `FF 03 24 01 01/00`
(495), reads 36 and 77 (111-112). cmd 77 reply (`PairPhoneBeanList`):
`[count, {len, status, mac[6], name utf8 NUL-terminated…}×]`; status 2/3 =
connected, 4 = connecting (app-side), others = paired but disconnected. cmd 79
(`PhoneStatusBean`) `[mac[6], status]`. Phone actions
(`QCYConnectManager.setPhoneBTAction`, 1607-1625): `FF 09 4D 07 action
mac[6]`, action 1 = connect (UI sets "connecting"), 2 and 3 are used for
disconnect / forget (exact mapping **ambiguous**). Note: when 113 precedes a
type-4 item with `cmdid 36`, that item is dropped (§2.1). Desktop: the switch
yes; phone list optional.

### 6.9 Layout 1000: generic list of hex-coded values (71 panels)

```json
{"type":1000,"cmdid":31,"itemName":{"0500":"5Minute","0A00":"10Minute","1E00":"30Minute","3C00":"60Minute","FFFF":"Don't shut down"},
 "subItem":[{"key":"0500","name":"5Minute"},…],"itemNameIos":[…],"itemKeyIos":[…],"icon":url,"desc"?}
```
`subItem[{key,name}]`, when present, overrides `itemName` (analyseJSON,
simple:711-736). `itemNameIos`/`itemKeyIos` unused on Android (and
19802 even has a numeric `500` there).

* Write (`v2ui/activity/SubItemListActivity.java:82`): key hex string →
  bytes → `setBytesValue(cmdid, bytes)` → `FF (n+2) cmdid n bytes`.
  E.g. 31 "30 min" → `FF 04 1F 02 1E 00`; never → `FF 04 1F 02 FF FF`.
* Read: `FF 03 FE 01 cmdid`; ≤ 2-byte reply → `SingleDataBean` (u16 LE);
  display key = `%02X` of the low byte if present in `itemName`, else
  `%02X%02X` (low, high) (`SingleKVItemView.java:476-500, 604-612`). So a key
  is matched by its byte sequence.

Values seen: cmd 31 `STANDBY` (auto power-off after disconnect, u16 LE
minutes, FFFF never; 63 panels); cmd 46 `MUSIC_MODE` (`01` music, `03`
game, `04` movie; 2 panels); cmd 51 (`0x33`, no constant in `DataBean`;
"Effective After Restart", `26` Medium, `2D` Normal, `34` High; 19794..19804
Crossky C30/T33/C50/CT06 etc.). The meaning of cmd 51 is not named in the
code; the values 0x26/0x2D/0x34 (38/45/52) look like a level (possibly
call-mic or volume limit) – **unknown, show the panel names only**.

### 6.10 Layout 1001: prompt language (cmd 25; 42 panels)

`{"type":1001,"cmdid":25,"itemKey":["57513030","57513031"],"itemName":["English","Chinese"],"languages":"zh","jump":"0"}`
Keys are hex of ASCII codes: `WQ00`/`WQ01` (WuQi), `JL00`/`JL01`/`JL02`
(JieLi; 4583 Era 5: `JL00` = "Tone sound", `JL01` English, `JL02`
Chinese), `LX00`/`LX01` (19797..19802). **The row is only shown when the
phone's language is contained in `languages`** (e.g. only Chinese phones),
but cmd 25 is always read. Write (`v2ui/activity/SubStringItemListActivity.java:84`):
`FF 06 19 04 57 51 30 30`. Read 25 (4 bytes → `CmdHexDataBean`, lowercase
hex) compared case-insensitively with `itemKey` in the settings list
(`SingleKVItemView.java:332-348`; the More-page variant compares
case-sensitively, `SingleButtonItemView.java:343-363`). Desktop: supported;
recommend showing it regardless of locale.

---
## 7. Other layouts and recommendations

### 7.1 Layout 3: "Find headset" (180 panels)

`{"type":3,"name":"Find headset","info":…,"latitude":"23.345","longitude":"116.102"}`
(coordinates are a constant placeholder). Becomes a viewType-8 row
(`FindEarbudsItemView`) that opens `ui/device/LocalMapActivity.java`: a map
of the phone's last location when the buds disconnected plus a "play sound"
button that plays `res/raw/didi2` **through the phone's media stream** at
half volume (379-400). No BLE command is sent. Desktop: optional — "play a
loud tone on the earbuds' audio sink"; no location.

### 7.2 Layout 8: L/R balance (cmd 22; 101 panels)

`{"type":8,"name":"Sound channel balance","info":…}` → `hasBalanceModel`.
Sound tab seek bar 0..100, 50 = centre; < 50 attenuates the **right**
channel (right % = 2·v), > 50 the left (left % = 2·(100−v))
(`v2ui/fragment/EQInfoFragment.java:124-176`); sent while dragging at most
every 200 ms and on release: `FF 03 16 01 v`; reset button sends 50
(210-218). Refused unless both buds report battery (or `isNotTWS`). Read:
`FF 03 FE 01 16` → `BalanceDataBean [v]` (also on every sound-tab resume).
Desktop: supported.

### 7.3 Layout 20: "Volume" (3 panels: 19842, 19873, 19921)

`{"type":20,"display":true,"name":"volume","title":"Volume"}` only sets
`hasVolumeModel`, which **no code reads** in 4.0.7 (`isHasVolumeModel`, no
callers); `VolumeModelView` changes the phone's own media volume. The
earbud-side volume command exists: `FF 05 08 03 L R 00`
(`VolumeDataBean.getVoiceCMD`; used only by the unreachable
`ImageInfoItemView`, 81), read 8 → `[L, R, max]`. Desktop: optional
("earbud volume" slider using cmd 8), low priority.

### 7.4 Layouts present in panels but phone-specific or JieLi-only

| type | what | desktop |
|---|---|---|
| 106 (19791) | local/focus music player: cmd 57 `FOCUS_MODE` (0 BT, 1 focus), 58 play state, 59 file list (`MusicFileBeanList`), `LocalMusicControlActivity`; cmd 55 `PLAY_MODE` (2 = SD/focus playback; 4 selects `aux_eq` presets; 255 ignored) | only the BT/Focus switch (`FF 03 39 01 00/01`); skip the player |
| 114 (32891/32894 JieLi) | head gestures: `actions[{keyID, keyName, events[{eventID, name, list[{cmd,name}], defaultFun, helpTip}]}]`, cmdid 80; write `FF 04 51 02 eventID cmd` (`setHeadAction`, cmd 81 `SENSOR_CONFIG`) | JieLi only; skip |
| 116 (32891/32894 JieLi) | spatial audio mode, cmdid 91: `subItem[{index, name, describe, mp3Url, previewImg}]` (1 close, 2 fixed, 3 head tracking); presumably `FF 03 5B 01 index` (same `ModeItemSelectView`-style selection; **not traced in detail**) | JieLi only; skip (non-JieLi spatial audio is the cmd 45 switch) |
| 115 | AI ANC, §3.6 | later |
| 11 | ANC percent, §3.4 | JieLi only in practice |
| 13, 24 | ignored by 4.0.7 | skip |
| 1 (item) | OTA | **skip (project rule)** |

### 7.5 Parser-supported types absent from all panels

105 LED effects (reads 53 `LED_SWITCH`, 54 `LED_EFFECT`; `LEDEffectActivity`,
`manager/controlpan/JSONBeanLEDEffect.java`; write `setLEDEffect` → cmd 54
bytes); 107 mode select (generic `ModeItemSelectView`, `FF 03 cmdid 01
index`); 108 white-noise player (`WhiteMusicControlView`); 109 alarms
(`EarphoneAlarmActivity`, cmd 63, clock sync cmd 62
`FF 09 3E 07 yy mm dd hh mi ss wdaybit`); 117 `AIPickingActivity`; 118
hearing health (`HearHealthActivityActivity`, cmd 88, value =
`b0<<24|b1<<16|b3<<8|b2`); 21 voice type; 7 and 12 ANC variants. Not needed
until a panel uses them.

---

## 8. Every `cmdid` (and related ids)

"In panels" = where it appears (L = layout, I = type-4 item). Write
examples are complete frames on `0x1001`.

| id (hex) | `DataBean` name | in panels | meaning | write | read / parser / value |
|---|---|---|---|---|---|
| 1 (01) | `RESET_DEFAULT` | item cmd `FF020100` | restore user settings | `FF 02 01 00` | – |
| 2 (02) | `CLEAR_PAIR` | item cmd `FF020200` | forget phones | `FF 02 02 00` | – |
| 3 (03) | `FACTORY_RESET` | item cmd `FF020300` | factory reset | `FF 02 03 00` | – |
| 4 (04) | `MUSICACTION` | – | play/pause/prev/next | `FF 03 04 01 1/2/3/4` | single; 1 = playing |
| 6 (06) | `RUER` | I200/I203 | in-ear detection (legacy bool) | `FF 03 06 01 01/02` | `RuerDataBean` `[0]==1` |
| 7 (07) | `NOISE` | – | legacy ANC strength (never written) | (`FF 03 07 01 v`) | `NoiseDataBean(0)` |
| 8 (08) | `VOICE` | (L20) | earbud volume | `FF 05 08 03 L R 00` | `[L, R, max]` |
| 9 (09) | `DIYANSHI` | I200/I203/I206 | gaming / low latency | `FF 03 09 01 01/02` | `DiyanshiDataBean` `[0]==1` |
| 10 (0A) | `JIANTING` | (item 202 default) | monitor/transparency V1 | char `0x000F` | `0x000F` byte 1 |
| 12 (0C) | `NOISEMODE` | L6 (via `cmdid` 1..4) | legacy ANC mode | `FF 03 0C 01 m` | `NoiseDataBean(1)` `[m]` |
| 13 (0D) | `TESTMODE` | L24 | touch test | `FF 03 0D 01 01/02` | `TestModeDataBean` |
| 16 (10) | `SLEEPMODE` | I200/I204 | sleep (touch off) | `FF 03 10 01 01/02` | `SleepDataBean` |
| 17 (11) | `COMPACTNESS` | L110 | fit test | `FF 05 11 03 e e e` | `[status, L, R]` |
| 18 (12) | `LEDMODE` | I200 | status LED on/off | `FF 03 12 01 01/02` | `LEDModeDataBean` |
| 20 (14) | `POWERMANAGER` | L102 | sleep timer | `FF 06 14 04 lo hi 00 00` | `[set16, left16]` |
| 22 (16) | `BALANCE` | (L8) | L/R balance | `FF 03 16 01 v` | `[v]` |
| 23 (17) | `ANCSETTING` | (L9) | ANC mode/scene/level | `FF 05 17 03 m s l` | `[m, s, l]` |
| 24 (18) | `PAIRNAME` | (L101) | BT name | `FF len 18 n utf8` | `PairnameDataBean` |
| 25 (19) | `VOICENAME` | L1001 | prompt language | `FF 06 19 04 a b c d` | hex string |
| 27 (1B) | – | I200 | APTX on/off (reboots) | `FF 03 1B 01 01/02` | single, 1 = on |
| 29 (1D) | `TONEVOLUME` | L103 | prompt volume | `FF 04 1D 02 v 00` (0x4161: `FF 03 1D 01 v`) | `[v, max]` |
| 31 (1F) | `STANDBY` | L1000 | auto off after disconnect | `FF 04 1F 02 lo hi` | u16 LE |
| 32 (20) | `MULTIEQ` | (L2) | parametric EQ, 6-byte bands | §4.2 | `EQParamListBean` |
| 34 (22) | `MULTIEQ2` | (L2) | parametric EQ, 7-byte bands | §4.2 | `EQParamListBean` |
| 35 (23) | `LDAC` | I200 | LDAC (reboots, disables multipoint) | `FF 03 23 01 01/02` (19805: `/00`) | single |
| 36 (24) | `MOREDEVICE` | I200, L113 | multipoint (reboots, disables LDAC) | `FF 03 24 01 01/02` (113: `01/00`) | single |
| 39 (27) | `AEQ` | I200 | dynamic/adaptive EQ | `FF 03 27 01 01/00` | single |
| 41 (29) | `ANC_WEAR` | – | ANC wear state | – | 1 byte duplicated |
| 42 (2A) | – | I200/I203 | anti-wind noise | `FF 03 2A 01 01/00` | single |
| 43 (2B) | `KEYFUN` | – | key table in a frame | – | pairs, §5.0 |
| 44 (2C) | `PEIDAI` | L104 | wear detection | `FF 05 2C 03 en music anc` | `[en, music, anc, tone]` |
| 45 (2D) | `SPACE_AUDIO` | I200 | spatial sound on/off | `FF 03 2D 01 01/00` | single |
| 46 (2E) | `MUSIC_MODE` | L1000, L111 | music/game/movie | `FF 03 2E 01 01/03/04` | single |
| 50 (32) | `ENV_ADAPTATION` | (L9 `envAdaptation`) | ANC environment adaptation | `FF 03 32 01 01/00` | single |
| 51 (33) | – | L1000 | unknown level (Medium 0x26 / Normal 0x2D / High 0x34; "effective after restart") | `FF 03 33 01 v` | single |
| 53 (35) / 54 (36) | `LED_SWITCH` / `LED_EFFECT` | (L105) | LED effects | cmd 54 bytes | single / `LEDEffectDataBean` |
| 55 (37) | `PLAY_MODE` | – | 2 = SD/focus playback, 4 = aux | – | single (255 ignored) |
| 57 (39) | `FOCUS_MODE` | L106 | BT (0) / focus (1) mode | `FF 03 39 01 00/01` | single |
| 64 (40) | – | I200/I203 | adaptive volume | `FF 03 40 01 01/00` | single |
| 68 (44) | `MAXEQ_COUNT` | – | max parametric bands | – | single; read 1 s after connect |
| 70/71 (46/47) | `MULTIEQ_LEFT/RIGHT` | – | per-side EQ (preset id 11) | like 34 | – |
| 72 (48) | `INEAR_SENSITIVITY` | L104 `sensitivity.cmdID` | wear sensor sensitivity | `FF 03 48 01 v` | single |
| 74 (4A) | `GAME_CONFIG` | L112 | game mode + sync EQ (reboots) | `FF 04 4A 02 en sync` | 2 bytes |
| 77 (4D) | `PHONE_INFO` | (L113) | paired phone list / action | `FF 09 4D 07 act mac6` | `PairPhoneBeanList` |
| 79 (4F) | `PHONE_STATUS` | – | phone status | – | `[mac6, status]` |
| 80 (50) / 81 (51) | `HEAD_ACTION` / `SENSOR_CONFIG` | L114 | head gestures | `FF 04 51 02 ev cmd` | `SensorFuncDataBean` |
| 83 (53) | – | I200 | eco mode (ANC off ≤ 20 %) | `FF 03 53 01 01/00` | single |
| 84 (54) | `AI_ANC` | L115 | AI ANC | `FF 08 54 06 (00/04) 00 00 00 00 01` | `AIANCSettingDataBean` |
| 85/86 (55/56), 90 (5A) | `AI_ANC_CFG/CFG2/PREVIEW` | – | AI ANC config | §3.6 | `AIANCConfigDataBean` |
| 87 (57) | – | I200 (JieLi) | bass enhancement | raw `570101/570100` | – |
| 88 (58) | `HEAR_HEALTH` | – | hearing health | – | special u32 |
| 91 (5B) | `SPACE_AUDIO_MODE` | L116 | spatial mode | – | single |
| 254 (FE) | `REQUESTDATA` | – | read request | `FF 03 FE 01 id` | – |

ids 27, 42, 51, 64, 83, 87 have no `CMDID_*` constant and no dedicated code:
they work purely through the generic panel paths (switch bytes / 1000 list),
which is exactly what a data-driven desktop UI should do.

---
## 9. Coverage: layout types per downloaded panel

Generated with `python3 -I` over `research/panel_json/models/*.json`
(186 panels with data). Notation: layout types in panel order; `(cNN)` =
layout `cmdid`; `4[...]` = item types of the type-4 list with `/cNN` item
`cmdid`; `none` = layout without `type` (ignored). "fw" = the server's
`firmwareversionGte..Lte` (all `0.0.0..999.999.999`). "Chip" is the
downloader's classification.

No panel (`data: null`, the app falls back to `controlpan_default.json` /
`controlpan_jl.json`): 6577 (0x19B1) QCY AilyPodsBuds Lite, 16737 (0x4161)
QCY Crossky Link, 16823 (0x41B7) HEROAD GT2, 18812 (0x497C) and 18814
(0x497E) QCY Ailypods Neo (JieLi), 23875 (0x5D43) CT03, 23885 (0x5D4D) HT27.

| vendorId | model | chip | layouts |
|---|---|---|---|
| 4577 (0x11E1) | C1 | Generic | 13 14 2 8 3 101 102(c20) 103(c29) 4[203/c9,3,3,100] |
| 4578 (0x11E2) | C2 | Generic | 13 14 2 8 3 101 102(c20) 103(c29) 4[203/c9,3,3,100] |
| 4579 (0x11E3) | QCY Crossky Link2 | Generic | 14 2 3 8 101 102(c20) 4[203/c9,1,3,100] none |
| 4580 (0x11E4) | QCY C1 OPEN | Generic | 13 14 2 8 3 101 102(c20) 103(c29) 4[203/c9,3,3,100] |
| 4581 (0x11E5) | QCY C1 CANAL | Generic | 13 14 2 8 3 101 102(c20) 103(c29) 4[203/c9,3,3,100] |
| 4583 (0x11E7) | QCY Era 5 | Generic | 14 9 2 3 101 1001(c25) 103(c29) 102(c20) 4[203/c9,200/c45,200/c36,1,3,100,100] |
| 6513 (0x1971) | QCY ArcBuds Lite | JieLi | 1 2 3 8 101 102(c20) 4[204/c16,206/c9,3,100] |
| 6514 (0x1972) | QCY AilyBuds Lite | JieLi | 1 2 3 8 101 102(c20) 1000(c31) 4[204/c16,206/c9,3,100] |
| 6515 (0x1973) | TWS QT29 | JieLi | 1 2 3 8 101 102(c20) 1000(c31) 4[204/c16,206/c9,3,100] |
| 6516 (0x1974) | TWS QT27 | JieLi | 1 2 3 8 101 102(c20) 4[204/c16,206/c9,3,100] |
| 6517 (0x1975) | QCY T29 APP | JieLi | 1 2 3 8 101 102(c20) 1000(c31) 4[204/c16,206/c9,3,100] |
| 6518 (0x1976) | QCY Buds | JieLi | 1 2 3 8 101 102(c20) 103(c29) 4[204/c16,206/c9,3,100] |
| 6519 (0x1977) | QCY T43 Buds | JieLi | 1 2 3 8 101 102(c20) 103(c29) 4[204/c16,206/c9,3,100] |
| 6520 (0x1978) | QCY T41 Motif | JieLi | 1 2 3 8 101 102(c20) 103(c29) 4[204/c16,206/c9,3,100] |
| 16738 (0x4162) | QCY-T22APP | Generic | 14 2 3 101 102(c20) 4[203/c9,1,3,100] |
| 16753 (0x4171) | H2 | JieLi | 2 3 101 4[206/c9,3,100] |
| 16754 (0x4172) | H2 Pro | JieLi | 14 2 3 8 101 102(c20) 4[206/c9,1,3,100] |
| 16755 (0x4173) | QCY H3 | JieLi | 14 2 9 3 1001(c25) 8 101 103(c29) 102(c20) 4[206/c9,1,3,100] |
| 16756 (0x4174) | QCY H2APP PRO HEADSET | JieLi | 14 2 3 8 101 102(c20) 4[206/c9,1,3,100] |
| 16757 (0x4175) | QCY H3 ANC HEADSET | JieLi | 14 2 9 3 8 101 102(c20) 4[206/c9,1,3,100] |
| 16758 (0x4176) | QCY H3 Lite | JieLi | 14 2 9 3 8 101 102(c20) 4[206/c9,1,3,100] |
| 16759 (0x4177) | QCY H3 Pro | JieLi | 14 2 9 3 8 101 1001(c25) 103(c29) 102(c20) 1000(c31) 4[206/c9,200/c35,200/c36,1,3,100] |
| 16760 (0x4178) | QCY H3 | JieLi | 14 2 9 3 1001(c25) 8 101 103(c29) 102(c20) 4[206/c9,1,3,100] |
| 16761 (0x4179) | QCY Era 7 | JieLi | 14 2 3 8 101 1001(c25) 103(c29) 102(c20) 1000(c31) 4[206/c9,1,3,100] |
| 16762 (0x417A) | QCY H3 Lite ANC APP | JieLi | 14 2 9 3 8 101 102(c20) 4[206/c9,1,3,100] |
| 16763 (0x417B) | QCY H3 PRO ANC | JieLi | 14 2 9 3 8 101 1001(c25) 103(c29) 102(c20) 1000(c31) 4[206/c9,200/c35,200/c36,1,3,100] |
| 16764 (0x417C) | QCY H3S | JieLi | 14 111(c46) 2 9 3 8 101 1001(c25) 103(c29) 102(c20) 1000(c31) 4[200/c35,200/c36,1,3,100] |
| 16765 (0x417D) | QCY H3S EXTREME ANC | JieLi | 14 111(c46) 2 9 3 8 101 1001(c25) 103(c29) 102(c20) 1000(c31) 4[200/c35,200/c36,1,3,100] |
| 16767 (0x417F) | QCY H3 ANC HEADSET | JieLi | 14 2 9 3 8 101 102(c20) 4[206/c9,1,3,100] |
| 16818 (0x41B2) | QCY Heroad V200 | Generic | 14 2 3 101 1001(c25) 103(c29) 102(c20) 1000(c31) 1000(c46) 4[1,3,100,100] |
| 16821 (0x41B5) | QCY Heroad VT200 | Generic | 14 2 3 101 1001(c25) 103(c29) 102(c20) 1000(c31) 1000(c46) 4[1,3,100,100] |
| 16822 (0x41B6) | QCY H2S Lite | Generic | 14 2 101 103(c29) 1000(c31) 102(c20) 4[203/c9,200/c36,1,3,100,100] 3 |
| 16824 (0x41B8) | QCY BARAM | Generic | 14 2 101 103(c29) 1000(c31) 102(c20) 4[203/c9,200/c36,1,3,100,100] 3 |
| 16849 (0x41D1) | QCY H4 | Generic | 14 2 9 8 3 101 102(c20) 4[203/c9,1,3,3,100] |
| 16850 (0x41D2) | QCY H4 ANC HEADSET | Generic | 14 2 9 8 3 101 102(c20) 4[203/c9,1,3,3,100] |
| 18289 (0x4771) | DT10 | JieLi | 1 2 11 3 8 101 4[204/c16,206/c9,1,3,100] |
| 18290 (0x4772) | T13 ANC | JieLi | 1 2 11 3 8 101 4[204/c16,206/c9,1,3,100] |
| 18293 (0x4775) | QCY Crossky GTR | JieLi | 1 2 3 8 101 102(c20) 4[204/c16,206/c9,1,3,100] |
| 18294 (0x4776) | QCY T13 ANC2 | JieLi | 1 2 11 3 8 101 102(c20) 1000(c31) 4[204/c16,206/c9,1,3,100] |
| 18295 (0x4777) | QCY T13 ANC2 APP | JieLi | 1 2 11 3 8 101 102(c20) 1000(c31) 4[204/c16,206/c9,1,3,100] |
| 18296 (0x4778) | QCY AilyBuds E10 | JieLi | 1 2 3 8 1001(c25) 103(c29) 102(c20) 1000(c31) 101 4[200/c35,200/c36,206/c9,204/c16,1,3,100] |
| 18297 (0x4779) | QCY AilyBuds E20 | JieLi | 1 2 9 3 8 101 103(c29) 1001(c25) 102(c20) 1000(c31) 4[203/c42,204/c16,206/c9,1,3,100] |
| 18298 (0x477A) | QCY MeloBuds N50 | JieLi | 1 2 9 3 8 101 1001(c25) 103(c29) 102(c20) 1000(c31) 4[204/c16,206/c9,200/c36,1,3,100] |
| 18299 (0x477B) | QCY Crossky C10 | JieLi | 1 2 3 8 102(c20) 1000(c31) 103(c29) 1001(c25) 101 4[206/c9,204/c16,200/c36,1,3,100] |
| 18300 (0x477C) | QCY HT12 MeloBuds N50 | JieLi | 1 2 9 3 8 101 1001(c25) 103(c29) 102(c20) 1000(c31) 4[204/c16,206/c9,200/c36,1,3,100] |
| 18301 (0x477D) | QCY CT01 APP | JieLi | 1 2 3 8 102(c20) 1000(c31) 103(c29) 1001(c25) 101 4[206/c9,204/c16,200/c36,1,3,100] |
| 18302 (0x477E) | QCY-T37 ANC AilyBuds | JieLi | 1 2 9 3 8 101 103(c29) 1001(c25) 102(c20) 1000(c31) 4[203/c42,204/c16,206/c9,1,3,100] |
| 18303 (0x477F) | QCY-T35 AilyBuds | JieLi | 1 2 3 8 1001(c25) 103(c29) 102(c20) 1000(c31) 101 4[200/c35,200/c36,206/c9,204/c16,1,3,100] |
| 18801 (0x4971) | T17 | JieLi | 1 2 3 4[204/c16,3,100] |
| 18802 (0x4972) | T17 | JieLi | 1 2 3 4[204/c16,3,100] |
| 18803 (0x4973) | T1C | JieLi | 1 2 4[204/c16,3,100] |
| 18804 (0x4974) | T19 | JieLi | 1 2 4[204/c16,206/c9,3,100] |
| 18805 (0x4975) | T2C | JieLi | 1 2 4[204/c16,3,100] |
| 18806 (0x4976) | AilyPods | JieLi | 1 2 3 4[204/c16,206/c9,3,100] |
| 18808 (0x4978) | DT08 | JieLi | 1 2 3 4[204/c16,206/c9,3,100] |
| 18809 (0x4979) | QCY FairyBuds | JieLi | 1 2 3 4[204/c16,206/c9,1,3,100] |
| 18810 (0x497A) | DT08 | JieLi | 1 2 3 4[204/c16,206/c9,3,100] |
| 18811 (0x497B) | DT08 | JieLi | 1 2 3 4[204/c16,206/c9,3,100] |
| 18813 (0x497D) | T1C Pro | JieLi | 1 2 3 4[204/c16,3,100] |
| 18815 (0x497F) | QCY T13X | JieLi | 1 2 3 8 101 102(c20) 4[204/c16,206/c9,1,3,100] |
| 18863 (0x49AF) | T2C | Generic | 1 2 3 4[3,100] |
| 18961 (0x4A11) | T18 | Generic | 10 2 3 4[204/c16,200/c27,203/c9,1,100] |
| 19057 (0x4A71) | T5 | JieLi | 1 2 3 4[1,3,100] |
| 19058 (0x4A72) | QT12 | JieLi | 1 2 3 4[1,3,100] |
| 19059 (0x4A73) | T13 | JieLi | 1 2 3 4[204/c16,1,3,100] |
| 19060 (0x4A74) | T7 | JieLi | 1 2 3 4[201,1,3,100] |
| 19061 (0x4A75) | T8 | JieLi | 1 2 3 4[201,1,3,100] |
| 19063 (0x4A77) | T5 | JieLi | 1 2 3 4[1,3,100] |
| 19064 (0x4A78) | T8 | JieLi | 1 2 3 4[201,1,3,100] |
| 19065 (0x4A79) | M18 | JieLi | 1 2 4[204/c16,1,3,100] |
| 19067 (0x4A7B) | T8S | JieLi | 1 2 3 4[201,1,3,100] |
| 19068 (0x4A7C) | T13 Pro | JieLi | 1 2 3 4[204/c16,1,3,100] |
| 19069 (0x4A7D) | T13 | JieLi | 1 2 3 4[204/c16,1,3,100] |
| 19071 (0x4A7F) | QCY Air | JieLi | 1 2 3 8 101 102(c20) 103(c29) 4[204/c16,206/c9,3,100] |
| 19218 (0x4B12) | QT16 | Generic | 10 2 3 4[203/c9,1,100,100] |
| 19219 (0x4B13) | T17S | Generic | 10 2 3 4[203/c9,1,100] |
| 19220 (0x4B14) | T18 | Generic | 10 2 3 8 102(c20) 4[204/c16,200/c27,203/c9,1,100] |
| 19221 (0x4B15) | DT06 | Generic | 10 2 4[203/c9,1,100] |
| 19313 (0x4B71) | AT02 | JieLi | 1 2 3 4[201,1,3,100] |
| 19393 (0x4BC1) | HT03 | Realtek | 10 6 2 3 4[203/c9,1,3,100] |
| 19394 (0x4BC2) | HT02 | Realtek | 10 6 2 3 4[1,3,100] |
| 19396 (0x4BC4) | HT03 | Realtek | 10 6 2 3 4[203/c9,1,3,100] |
| 19397 (0x4BC5) | HT02 | Realtek | 10 6 2 3 4[1,3,100] |
| 19489 (0x4C21) | G1 | Generic | 10 2 23 4[203/c9,200/c16,200/c18,1,3,4] |
| 19569 (0x4C71) | T8 | JieLi | 1 2 3 4[201,1,3,100] |
| 19570 (0x4C72) | QT12 | JieLi | 1 2 3 4[1,3,100] |
| 19571 (0x4C73) | T5 | JieLi | 1 2 3 4[1,3,100] |
| 19572 (0x4C74) | M18 | JieLi | 1 2 3 4[201,1,3,100] |
| 19573 (0x4C75) | T7 | JieLi | 1 2 3 4[201,1,3,100] |
| 19574 (0x4C76) | T8 | JieLi | 1 2 3 4[201,1,3,100] |
| 19575 (0x4C77) | M18 | JieLi | 1 2 3 4[201,1,3,100] |
| 19576 (0x4C78) | T8 | JieLi | 1 2 3 4[201,1,3,100] |
| 19649 (0x4CC1) | HT01C | Realtek | 10 6 2 3 4[203/c6,203/c9,1,100,100] |
| 19730 (0x4D12) | T1X | Generic | 1 2 3 4[3,100] |
| 19732 (0x4D14) | QT10S | Generic | 1 2 3 4[3,100] |
| 19733 (0x4D15) | T4S | Generic | 1 2 3 4[3,100] |
| 19735 (0x4D17) | QCY-QT11S | Generic | 1 10 2 3 4[203/c6,203/c9,1,100] |
| 19745 (0x4D21) | T5 | Generic | 1 2 3 4[1,2,3,100] |
| 19746 (0x4D22) | T5S | Generic | 1 2 3 4[201,1,3,100] |
| 19747 (0x4D23) | T6 | Generic | 1 2 3 4[1,3,100] |
| 19748 (0x4D24) | T3 | Generic | 1 2 3 4[1,3,100] |
| 19749 (0x4D25) | T5 Pro | Generic | 1 2 3 4[1,3,100] |
| 19751 (0x4D27) | T4 | Generic | 1 2 3 4[1,3,100] |
| 19761 (0x4D31) | T4 | Generic | 1 2 3 4[1,3,100] |
| 19762 (0x4D32) | T1C | Generic | 1 2 3 4[3,100] |
| 19763 (0x4D33) | T1S | Generic | 1 2 3 4[3,100] |
| 19764 (0x4D34) | T9S | Generic | 1 2 3 4[3,100] |
| 19765 (0x4D35) | T6 | Generic | 1 2 3 4[1,3,100] |
| 19777 (0x4D41) | Melobuds ANC | WuQi | 10 9 2 8 3 101 4[203/c9,200/c16,1,3,100] |
| 19778 (0x4D42) | TWS HT05 | WuQi | 10 9 2 8 3 101 4[203/c9,200/c16,1,3,100] |
| 19779 (0x4D43) | QCY ArcBuds | WuQi | 10 9 2 8 3 101 102(c20) 4[203/c9,200/c16,1,3,100] |
| 19780 (0x4D44) | QCY Crossky Link2 | WuQi | 14 2 3 8 101 102(c20) 103(c29) 1000(c31) 4[203/c9,1,3,100] |
| 19782 (0x4D46) | TWS HT07 | WuQi | 1 2 3 8 101 102(c20) 4[204/c16,206/c9,3,100] none |
| 19783 (0x4D47) | QCY HT07 ANC APP | WuQi | 10 9 2 8 3 101 102(c20) 4[203/c9,200/c16,1,3,100] |
| 19784 (0x4D48) | QCY AilyBuds Pro | WuQi | 10 9 2 8 3 101 103(c29) 102(c20) 4[203/c42,203/c9,200/c16,1,3,100] |
| 19785 (0x4D49) | HT08 | WuQi | 10 9 2 8 3 101 102(c20) 4[203/c9,200/c6,200/c16,200/c35,200/c36,1,3,100] |
| 19786 (0x4D4A) | QCY MeloBuds Pro | WuQi | 10 9 2 8 1001(c25) 103(c29) 101 102(c20) 104(c44) 1000(c31) 4[203/c9,200/c16,200/c35,200/c36,1,3,100] 3 |
| 19787 (0x4D4B) | QCY AilyBuds Pro+ | WuQi | 10 9 2 8 3 103(c29) 101 102(c20) 4[203/c42,200/c35,200/c36,203/c9,200/c16,1,3,100] |
| 19788 (0x4D4C) | QCY HT10ANC APP | WuQi | 10 9 2 8 3 101 103(c29) 102(c20) 4[203/c42,203/c9,200/c16,1,3,100] |
| 19789 (0x4D4D) | QCY HT10 Hires ANC APP | WuQi | 10 9 2 8 3 103(c29) 101 102(c20) 4[203/c42,200/c35,200/c36,203/c9,200/c16,1,3,100] |
| 19790 (0x4D4E) | QCY HT08 MeloBuds Pro Plus | WuQi | 10 9 2 8 1001(c25) 103(c29) 101 104(c44) 102(c20) 1000(c31) 4[203/c9,200/c16,200/c35,200/c36,1,3,100] 3 |
| 19791 (0x4D4F) | QCY Crossky R70 | WuQi | 10 2 8 106(c57) 3 1001(c25) 101 102(c20) 103(c29) 4[203/c9,200/c16,200/c35,200/c36,1,3,100] |
| 19794 (0x4D52) | QCY Crossky C30 | Generic | 10 2 8 1000(c51) 103(c29) 101 102(c20) 1000(c31) 4[203/c9,200/c45,200/c16,1,3,100] 3 |
| 19795 (0x4D53) | QCY T33 APP | Generic | 10 2 8 1000(c51) 103(c29) 101 102(c20) 1000(c31) 4[203/c9,200/c45,200/c16,1,3,100] 3 |
| 19796 (0x4D54) | QCY Crossky C30S | Generic | 10 2 8 1000(c51) 103(c29) 101 102(c20) 1000(c31) 4[203/c9,200/c45,200/c35,200/c16,1,3,100] 3 |
| 19797 (0x4D55) | QCY Crossky C50 | Generic | 10 2 8 1000(c51) 1001(c25) 103(c29) 101 102(c20) 1000(c31) 4[203/c64,203/c9,200/c39,200/c16,200/c36,1,3,100,100] 3 |
| 19798 (0x4D56) | QCY Crossky R50 | Generic | 10 2 8 1001(c25) 103(c29) 101 102(c20) 1000(c31) 4[203/c64,203/c9,200/c16,200/c36,1,3,100,100] 3 |
| 19799 (0x4D57) | QCY Crossky C50 | Generic | 10 2 8 1000(c51) 1001(c25) 103(c29) 101 102(c20) 1000(c31) 4[203/c64,203/c9,200/c39,200/c16,200/c36,1,3,100,100] 3 |
| 19801 (0x4D59) | QCY CT06 BlueTooth 6.0 | Generic | 10 2 8 1000(c51) 1001(c25) 103(c29) 101 102(c20) 1000(c31) 4[203/c64,203/c9,200/c39,200/c16,200/c36,1,3,100,100] 3 |
| 19802 (0x4D5A) | QCY RACE | Generic | 10 2 8 1001(c25) 103(c29) 101 102(c20) 1000(c31) 4[203/c64,203/c9,200/c16,200/c36,1,3,100,100] 3 |
| 19803 (0x4D5B) | QCY Crossky C50S | Generic | 10 2 8 1000(c51) 103(c29) 101 102(c20) 1000(c31) 4[200/c35,203/c64,203/c9,200/c39,200/c16,200/c36,1,3,100,100] 3 |
| 19804 (0x4D5C) | QCY Crossky C50S | Generic | 10 2 8 1000(c51) 103(c29) 101 102(c20) 1000(c31) 4[200/c35,203/c64,203/c9,200/c39,200/c16,200/c36,1,3,100,100] 3 |
| 19805 (0x4D5D) | QCY MeloBuds A30 | Generic | 10 2 9 3 8 101 103(c29) 102(c20) 1000(c31) 110(c17) 112(c74) 113(c36) 115(c84) 4[200/c42,200/c83,200/c16,200/c35,200/c39,1,3,100,100] |
| 19825 (0x4D71) | T7 | JieLi | 1 2 3 4[201,3,100] |
| 19826 (0x4D72) | T8 | JieLi | 1 2 3 4[201,3,100] |
| 19841 (0x4D81) | T7S | Generic | 1 2 3 4[200,1,3,100] |
| 19842 (0x4D82) | T8S | Generic | 20 10 1 2 24(c13) 3 4[203/c6,1,3,100] |
| 19846 (0x4D86) | QT12 | Generic | 1 2 3 4[1,3,100] |
| 19857 (0x4D91) | T9 | ZR | 1 2 3 4[1,3,100] |
| 19858 (0x4D92) | T7 | ZR | 1 2 3 4[201,1,3,100] |
| 19859 (0x4D93) | T8 | ZR | 1 2 3 4[201,1,3,100] |
| 19860 (0x4D94) | T1C | ZR | 1 2 3 4[3,100] |
| 19861 (0x4D95) | M10 | ZR | 1 2 3 4[3,100] |
| 19862 (0x4D96) | T1S | ZR | 1 2 3 4[3,100] |
| 19863 (0x4D97) | T2C | ZR | 1 2 3 4[3,100] |
| 19865 (0x4D99) | QT12S | ZR | 10 2 3 4[1,100,100] |
| 19866 (0x4D9A) | T13 | ZR | 1 2 3 4[204/c16,1,3,100] |
| 19873 (0x4DA1) | HT01 | Generic | 20 10 6 3 4[203/c6,203/c9,1,100,100] |
| 19889 (0x4DB1) | T8 | Generic | 1 2 3 4[201,3,100] |
| 19905 (0x4DC1) | QT10 | Realtek | 10 2 3 4[1,100,100] |
| 19906 (0x4DC2) | QT11 | Realtek | 10 2 3 4[203/c9,1,100,100] |
| 19908 (0x4DC4) | QT12S | Realtek | 10 2 3 4[1,100,100] |
| 19909 (0x4DC5) | QT10 Pro | Realtek | 10 2 3 4[203/c9,1,100,100] |
| 19921 (0x4DD1) | HT03S | Generic | 20 10 6 2 8 4[203/c9,200/c16,301,1,3,100] 3 |
| 22897 (0x5971) | QCY T13X APP | JieLi | 1 2 3 8 101 102(c20) 4[204/c16,206/c9,1,3,100] |
| 23153 (0x5A71) | QCY T13X | JieLi | 1 2 3 8 101 102(c20) 4[204/c16,206/c9,1,3,100] |
| 23154 (0x5A72) | QCY Crossky GTR2 | JieLi | 1 2 3 8 102(c20) 1000(c31) 101 4[206/c9,204/c16,1,3,100] |
| 23155 (0x5A73) | QCY MeloBuds Neo | JieLi | 1 2 3 8 101 102(c20) 4[204/c16,206/c9,1,3,100] |
| 23156 (0x5A74) | TWS QT26 | JieLi | 1 2 3 8 101 102(c20) 4[204/c16,206/c9,1,3,100] |
| 23157 (0x5A75) | QCY AilyBuds Clear | JieLi | 1 2 3 4[204/c16,206/c9,1,3,100] |
| 23158 (0x5A76) | QCY T13X APP | JieLi | 1 2 3 8 101 102(c20) 4[204/c16,206/c9,1,3,100] |
| 23159 (0x5A77) | QCY T30 AIR | JieLi | 1 2 3 8 102(c20) 1000(c31) 101 4[206/c9,204/c16,1,3,100] |
| 23160 (0x5A78) | QCY T31 MeloBuds Neo | JieLi | 1 2 3 8 101 102(c20) 4[204/c16,206/c9,1,3,100] |
| 23872 (0x5D40) | QCY MeloBuds N70 | WuQi | 10 9 2 8 3 1001(c25) 101 102(c20) 110(c17) 104(c44) 103(c29) 1000(c31) 4[203/c9,200/c42,200/c16,200/c35,200/c36,1,3,100] |
| 23873 (0x5D41) | QCY AilyBuds E50 | WuQi | 10 9 2 8 3 1001(c25) 101 102(c20) 110(c17) 104(c44) 103(c29) 1000(c31) 4[203/c9,200/c42,200/c16,200/c45,200/c35,200/c36,1,3,100] |
| 23874 (0x5D42) | QCY MeloBuds N60 | WuQi | 10 9 2 8 3 1001(c25) 101 102(c20) 110(c17) 104(c44) 103(c29) 1000(c31) 4[203/c9,200/c42,200/c16,200/c35,200/c36,1,3,100] |
| 23877 (0x5D45) | QCY MeloBuds N70 | WuQi | 10 9 2 8 3 1001(c25) 101 102(c20) 110(c17) 104(c44) 103(c29) 1000(c31) 4[203/c9,200/c42,200/c16,200/c35,200/c36,1,3,100] |
| 23878 (0x5D46) | QCY AilyBuds E50 | WuQi | 10 9 2 8 3 1001(c25) 101 102(c20) 110(c17) 104(c44) 103(c29) 1000(c31) 4[203/c9,200/c42,200/c16,200/c45,200/c35,200/c36,1,3,100] |
| 23879 (0x5D47) | QCY MeloBuds N60 | WuQi | 10 9 2 8 3 1001(c25) 101 102(c20) 110(c17) 104(c44) 103(c29) 1000(c31) 4[203/c9,200/c42,200/c16,200/c35,200/c36,1,3,100] |
| 23880 (0x5D48) | QCY MeloBuds N70i | WuQi | 10 9 2 8 3 1001(c25) 101 102(c20) 110(c17) 104(c44) 103(c29) 1000(c31) 4[203/c9,200/c42,200/c16,200/c35,200/c36,1,3,100] |
| 23881 (0x5D49) | QCY MeloBuds N70i | WuQi | 10 9 2 8 3 1001(c25) 101 102(c20) 110(c17) 104(c44) 103(c29) 1000(c31) 4[203/c9,200/c42,200/c16,200/c35,200/c36,1,3,100] |
| 23883 (0x5D4B) | QCY HT19 MeloBuds DUAL BT6 | WuQi | 10 9 2 8 3 1001(c25) 101 102(c20) 110(c17) 104(c44) 103(c29) 1000(c31) 4[203/c9,200/c42,200/c16,200/c35,200/c36,1,3,100] |
| 32881 (0x8071) | QCY MeloBuds N30 | JieLi | 1 2 9 3 8 101 103(c29) 102(c20) 1000(c31) 1001(c25) 4[204/c16,206/c9,200/c36,1,3,100,100] |
| 32882 (0x8072) | QCY Buds ANC | JieLi | 1 2 9 3 8 101 103(c29) 102(c20) 1000(c31) 1001(c25) 4[204/c16,206/c9,200/c36,1,3,100,100] |
| 32883 (0x8073) | QCY HT16 MeloBuds | JieLi | 1 2 9 3 8 101 103(c29) 102(c20) 1000(c31) 1001(c25) 4[204/c16,206/c9,200/c36,1,3,100,100] |
| 32884 (0x8074) | QCY MeloBuds N20 | JieLi | 1 2 9 3 8 101 103(c29) 102(c20) 1000(c31) 110(c17) 112(c74) 1001(c25) 4[204/c16,200/c36,1,3,100,100] |
| 32885 (0x8075) | QCY T13 Plus | JieLi | 1 2 9 3 8 101 103(c29) 102(c20) 1000(c31) 110(c17) 1001(c25) 4[204/c16,206/c9,200/c36,1,3,100,100] |
| 32886 (0x8076) | QCY MeloBuds A30 | JieLi | 1 2 9 3 8 103(c29) 102(c20) 1000(c31) 110(c17) 112(c74) 113(c36) 115(c84) 4[203/c42,200/c83,204/c16,200/c35,200/c39,1,3] |
| 32887 (0x8077) | QCY T13 Pro | JieLi | 1 2 9 3 8 101 103(c29) 102(c20) 1000(c31) 110(c17) 112(c74) 4[204/c16,200/c36,1,3,100,100] |
| 32889 (0x8079) | QCY MeloBuds N20 Pro | JieLi | 1 2 9 3 8 101 103(c29) 102(c20) 1000(c31) 110(c17) 112(c74) 1001(c25) 113(c36) 4[204/c16,200/c35,1,3,100,100] |
| 32890 (0x807A) | QCY Crossky C50i | JieLi | 1 2 3 8 103(c29) 102(c20) 1000(c31) 101 4[200/c36,200/c64,200/c87,200/c35,200/c9,1,3,100,100] |
| 32891 (0x807B) | QCY MeloBuds N65 | JieLi | 1 2 9 3 8 103(c29) 102(c20) 1000(c31) 110(c17) 112(c74) 113(c36) 114(c80) 115(c84) 116(c91) 4[204/c16,200/c36,200/c35,200/c39,1,3,100,100] |
| 32892 (0x807C) | QCY HT20 MeloBuds NEO BT6 | JieLi | 1 2 9 3 8 101 103(c29) 102(c20) 1000(c31) 110(c17) 112(c74) 1001(c25) 4[204/c16,200/c36,1,3,100,100] |
| 32893 (0x807D) | QCY MeloBuds A30 | JieLi | 1 2 9 3 8 103(c29) 102(c20) 1000(c31) 110(c17) 112(c74) 113(c36) 115(c84) 4[203/c42,200/c83,204/c16,200/c35,200/c39,1,3] |
| 32894 (0x807E) | QCY MeloBuds N65 | JieLi | 1 2 9 3 8 103(c29) 102(c20) 1000(c31) 110(c17) 112(c74) 113(c36) 114(c80) 115(c84) 116(c91) 4[204/c16,200/c36,200/c35,200/c39,1,3,100,100] |
| 36977 (0x9071) | QCY T13 PRO ANC BT6 | JieLi | 1 2 9 3 8 101 103(c29) 102(c20) 1000(c31) 110(c17) 112(c74) 4[204/c16,200/c36,1,3,100,100] |

---

## 10. Implementation notes for a data-driven desktop UI

1. **Parse leniently.** Coerce numbers from strings/floats; treat a missing
   or unknown `type` as "ignore"; accept `items` that are not objects.
2. **Route by chip first.** JieLi (`(vid & 0xF0) == 0x70` or 512) and ZR
   (`0x90`) panels use the same JSON but different transports; the raw
   (unframed) switch bytes and eventIDs ≥ 256 only make sense there.
3. **Build the read list like the app:** after connect read `0x1002`
   (dump), `0x0007`, `0x000D` (if present), the EQ (§4.4), cmd 68, and
   `FF 03 FE 01 id` for every id the panel contributes (§0.2 table), plus 22
   if type 8 exists. Space writes ≥ 20 ms apart.
4. **Settings list order** = panel order: type-14 block, key rows, then
   `otherSettingJson` in panel order, More page = 101 + items with
   `jump == 1`. (`otherSingleJson`, where 200–299 switches are inserted at
   the front, and `otherDitallJson` have no readers in 4.0.7.)
5. **Switches:** write the panel's bytes for framed `FF…` commands; read
   state as `value == 1`. Exception: type 206 non-JieLi (§2.2 rule 3).
6. **Lists (1000/1001/103):** write keys exactly as the app does; match
   replies by byte sequence / decimal key / case-insensitive hex.
7. **ANC:** implement §3.0 packing and §3.1 view types 1/2/3 (4 optional);
   respect ≥ 0.5–1.5 s between ANC writes; require both buds for scene
   changes like the app.
8. **EQ:** three families (§4.1); presets by `id` (or 1-based index); custom
   curve via the layout's own freq/q/iitType with auto master gain.
9. **Keys:** V2 `0x000D ← [eventID, action]`; V1 one byte per gesture
   characteristic; labels from the panel; action 0 → `0xFF` quirk for
   vendor nibble 0x2/0x3/0x8 with fw ≥ 1.1.0.
10. **Skip:** OTA (item type 1), find-my-phone map, local music player,
    AI-ANC configuration, head gestures, hearing health, market EQs.

### Ambiguities and risks (explicit)

* **Type 206 "Gaming mode" carries sleep-mode bytes** (`FF0310…`). On the one
  non-JieLi panel with it (19782 WuQi HT07) the official app turns on sleep
  mode (touch disabled) when the user enables gaming mode. Use cmd 9 there.
* **V1 long-press characteristics collide with V1 language (`0x0009`) and
  V1 reset/clear-pairing (`0x000A`) characteristics.** Only JieLi panels use
  them for keys; never write key bytes to these on other devices — `0x000A`
  `01/02/03` = restore/clear pairing/factory reset.
* **Power-off timer value 0 powers the earbuds off immediately** (it is the
  app's "Shut down" button); use `0xFFFF` for "off".
* APTX (27), LDAC (35), multipoint (36), game config (74) and cmd 51 reboot
  the earbuds (panel `alert`/`msg`/`desc` texts say so).
* cmd 51 meaning is unknown (values 0x26/0x2D/0x34 labelled
  Medium/Normal/High).
* EQ custom index 255 vs 128 (app sends 128 after the first custom curve) –
  firmware handling unknown. `iitType` value 4 assumed low shelf.
* Panels with `freq "62,250,1K,4K,8K"` lose the "K" bands in the official
  app (parse error) – the intended frequencies are obvious, but sending all
  five bands deviates from the app's behaviour.
* AI ANC config bytes (cmd 85/86/90) and phone actions 2/3 (cmd 77) were
  not fully traced.
* Item type 2 (V1 language picker on 19745 T5) is broken in 4.0.7 (sends
  an empty write); the intended write is the 1-byte code to `0x0009`.
* The app reads the V1 in-ear state (char `0x000F`) into a cache key the
  switch never reads; state display for that switch is effectively
  write-only in the official app.
* Layout types 13, 20 (volume) and 24 are present in panels but unused by
  4.0.7; `voice` key sections, `repeat`, `hasValue`, `supportMaxCount`,
  `modes` (in EQ), `itemNameIos/itemKeyIos`, `sys`, `leftImg/rightImg` are
  not read.
* All conclusions come from static reading of the decompiled code; nothing
  was verified on hardware other than what `docs/PROTOCOL.md` already
  records for the MeloBuds Pro.
