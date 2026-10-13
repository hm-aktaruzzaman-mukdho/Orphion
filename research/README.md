# Research material

Everything used to reverse-engineer the protocol, kept so the work can be
resumed without starting over. None of it is needed to build or run the app.

| Path              | Content |
|-------------------|---------|
| `../QCY_4.0.7_APKPure.apk` | The analysed app (QCY 4.0.7, build 717) |
| `apk_extracted/`  | The APK unzipped (dex, resources, native libs, `.proto` files) |
| `decompiled/`     | Java sources and resources produced by jadx 1.5.6 |
| `tools/jadx/`     | jadx 1.5.6 from the Kali repository (`bin/jadx`, `bin/jadx-gui`) |
| `decompiled_simple/` | Single classes re-decompiled with `jadx -m simple` where the normal output was garbled |
| `panel_json/`     | QCY's control panels: `models/<id>.json` for every earphone (API response + chip family), `catalog/` the product list, `defaults/` the app's built-in fallback panels. Refreshed by `scripts/update-panels.py --fetch`, which also rebuilds the bundle compiled into `qcy-protocol` |
| `notes/`          | Protocol write-ups: `panel_semantics.md` (the panel format and every layout/item/command id), `jieli.md` (JieLi RCSP as QCY uses it), `zr.md` (ZR chips), `web_research.md` (public sources, with licences) |
| `web_sources/`    | Local copies of the public sources cited in `notes/web_research.md` (not in git: mixed licences) |
| `logs/`           | bluetoothctl captures and `qcyctl -vv` protocol traces from real earbuds |
| `screenshots/`    | GUI captures (`qcy-gui --screenshots research/screenshots`) |

## Regenerating the decompiled sources

```bash
research/tools/jadx/bin/jadx -j 8 --show-bad-code -d research/decompiled QCY_4.0.7_APKPure.apk
```

Or browse interactively with `research/tools/jadx/bin/jadx-gui QCY_4.0.7_APKPure.apk`.

## Where the interesting code is

All paths below are relative to `decompiled/sources/`.

| File | What it shows |
|------|---------------|
| `com/qcymall/qcylibrary/QCYHeadsetClient.java` | GATT UUIDs, every command builder, notification dispatch |
| `com/qcymall/qcylibrary/dataBean/DataAnalyse.java` | Frame encoding / decoding |
| `com/qcymall/qcylibrary/dataBean/DataBean.java` | All command ids (`CMDID_*`) and the id → parser table |
| `com/qcymall/qcylibrary/dataBean/*DataBean.java` | Payload layouts of individual settings |
| `com/qcymall/earphonesetup/manager/QCYConnectManager.java` | Chip-family routing, EQ sending (`setEQData`, `getMaxGain`), on-connect sequence |
| `com/qcymall/earphonesetup/model/Devicebind.java` | Advertisement parsing, `is*Device()` chip checks |
| `com/qcymall/earphonesetup/manager/BleScanManager.java` | BLE scan filter (company id 21020) |
| `com/qcymall/earphonesetup/manager/controlpan/ControlpanelJSONManager.java` | How the per-model panel JSON drives the UI |
| `com/qcymall/earphonesetup/v3ui/mamager/WatchHttpAPI.java` | `product/findControlPanels` request |
| `com/qcymall/utils/equtils/Filter.java`, `FrequencyResponse.java` | EQ biquad maths |
| `com/qcymall/earphonesetup/v2ui/activity/InearConfigActivity.java` | Wear detection semantics |
| `com/qcymall/earphonesetup/v2ui/fragment/EQInfoFragment.java` | ANC / transparency / voice UI logic |

## Quick manual probing

```bash
bluetoothctl                       # then: scan le → look for ManufacturerData 0x521c
qcyctl -vv status                  # full hex trace of reads/writes/notifications
qcyctl raw ff03fe0117              # ask for the ANC setting (0x17); reply shows in the trace
```

Do **not** write to the WuQi OTA service (`0x7033`). Firmware updates are out
of scope for this project.
