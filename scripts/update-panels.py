#!/usr/bin/env python3
"""Fetch QCY control panels and build the bundle compiled into qcy-protocol.

The official app downloads a "control panel" JSON per model and builds its UI
from it. This script keeps an offline copy of all of them:

    scripts/update-panels.py --fetch    # download catalogue + every panel
    scripts/update-panels.py            # rebuild the bundle from the copies

Downloads go to research/panel_json/ (one file per model id, plus the product
catalogue). The bundle, crates/qcy-protocol/data/panels.json.zz, is a zlib
compressed JSON document:

    {
      "defaults": {"qcy": <panel>, "jieli": <panel>},
      "models": [{"id": 19786, "name": "QCY MeloBuds Pro", "panel": <panel>}, ...]
    }

`panel` is the `controlPanel` object exactly as served (or null when QCY has
none for the model; the defaults then apply, like in the app). The defaults
are the app's built-in res/raw/controlpan_{default,jl}.json, copied into
research/panel_json/defaults/ because the decompiled app is not in git.

Only the Python standard library is used. Network access happens only with
--fetch, and only to QCY's public product API, with no account.
"""

import argparse
import io
import json
import pathlib
import re
import sys
import time
import urllib.parse
import urllib.request
import zipfile
import zlib

ROOT = pathlib.Path(__file__).resolve().parent.parent
PANELS = ROOT / "research" / "panel_json"
MODELS = PANELS / "models"
CATALOG = PANELS / "catalog"
DEFAULTS = PANELS / "defaults"
BUNDLE = ROOT / "crates" / "qcy-protocol" / "data" / "panels.json.zz"

API = "https://api.watch.qcy.com/"
# "999.0.0" makes the server return the newest panel of every model.
NEWEST_FIRMWARE = "999.0.0"

# The catalogue zip comes from QCY's image server, which only serves plain
# HTTP (its certificate does not cover the host). It is accepted only from
# QCY's own domain, and only provides model names and ids; the panels
# themselves are fetched from the API over HTTPS.
CATALOG_HOST = re.compile(r"(^|\.)qcymall\.com$")
CATALOG_NAME = re.compile(r"^[A-Za-z0-9._-]+\.zip$")
# Limits against oversized answers and zip bombs.
MAX_DOWNLOAD = 32 * 1024 * 1024
MAX_JSON = 64 * 1024 * 1024


def read_limited(resp, limit=MAX_DOWNLOAD):
    data = resp.read(limit + 1)
    if len(data) > limit:
        sys.exit(f"answer from {resp.geturl()} is larger than {limit} bytes")
    return data


def post(path, fields):
    data = urllib.parse.urlencode(fields).encode()
    req = urllib.request.Request(API + path, data=data, method="POST")
    req.add_header("Content-Type", "application/x-www-form-urlencoded")
    with urllib.request.urlopen(req, timeout=30) as resp:
        return json.loads(read_limited(resp))


def catalog_url(url):
    """Checks a catalogue URL from the API; returns it and its file name."""
    parts = urllib.parse.urlsplit(url)
    name = parts.path.rsplit("/", 1)[-1]
    if (
        parts.scheme not in ("http", "https")
        or not CATALOG_HOST.search(parts.hostname or "")
        or not CATALOG_NAME.match(name)
    ):
        sys.exit(f"refusing unexpected catalogue URL {url!r}")
    return url, name


def chip(vendor_id):
    """Chip family, as decided by the app (Devicebind.is*Device)."""
    if (vendor_id & 0xF0) == 0x70 or vendor_id == 512:
        return "JieLi"
    return {0x40: "WuQi", 0x90: "ZR", 0xC0: "Realtek"}.get(vendor_id & 0xF0, "Generic")


def fetch_catalog():
    """Downloads the product catalogue zip and returns its earphone list."""
    listing = post("product/findProductList", {})
    url = next(
        (v for v in json.dumps(listing).split('"') if v.endswith(".zip")), None
    )
    if url is None:
        sys.exit(f"no catalogue URL in findProductList answer: {listing}")
    url, name = catalog_url(url)
    with urllib.request.urlopen(url, timeout=60) as resp:
        blob = read_limited(resp)
    CATALOG.mkdir(parents=True, exist_ok=True)
    (CATALOG / name).write_bytes(blob)
    with zipfile.ZipFile(io.BytesIO(blob)) as z:
        # Only read (never extract) the member, after checking its size.
        info = next(i for i in z.infolist() if i.filename.endswith(".json"))
        if info.file_size > MAX_JSON:
            sys.exit(f"catalogue member {info.filename} is too large")
        catalog = json.loads(z.read(info))
    (CATALOG / name.replace(".zip", ".json")).write_text(
        json.dumps(catalog, ensure_ascii=False, indent=1)
    )
    return catalog["earphones"]


def fetch_panels(earphones):
    MODELS.mkdir(parents=True, exist_ok=True)
    for product in earphones:
        vid = int(product["vendorID"])
        if not 0 <= vid <= 0xFFFF:
            print(f"skipping model id {vid!r}: not a 16-bit id")
            continue
        answer = post(
            "product/findControlPanels",
            {"modelId": vid, "firmwareVersion": NEWEST_FIRMWARE},
        )
        record = {
            "model": product["title"],
            "vendorID": vid,
            "chip": chip(vid),
            "response": answer,
        }
        (MODELS / f"{vid}.json").write_text(
            json.dumps(record, ensure_ascii=False, indent=1)
        )
        data = answer.get("data") if isinstance(answer, dict) else None
        has = bool(data and data.get("controlPanel"))
        print(f"{vid:6} {record['chip']:8} {product['title']:32} panel={has}")
        time.sleep(0.2)  # be gentle with QCY's server


def latest_catalog():
    files = sorted(CATALOG.glob("en_prod_*.json"))
    if not files:
        sys.exit("no catalogue in research/panel_json/catalog; run with --fetch")
    return json.loads(files[-1].read_text())["earphones"]


def build_bundle():
    names = {int(p["vendorID"]): p["title"] for p in latest_catalog()}
    models = []
    for path in sorted(MODELS.glob("*.json"), key=lambda p: int(p.stem)):
        record = json.loads(path.read_text())
        vid = int(record["vendorID"])
        answer = record["response"]
        data = answer.get("data") if isinstance(answer, dict) else None
        panel = (data or {}).get("controlPanel")
        models.append(
            {"id": vid, "name": names.get(vid, record["model"]).strip(), "panel": panel}
        )
    defaults = {
        "qcy": json.loads((DEFAULTS / "controlpan_default.json").read_text()),
        "jieli": json.loads((DEFAULTS / "controlpan_jl.json").read_text()),
    }
    doc = json.dumps(
        {"defaults": defaults, "models": models},
        ensure_ascii=False,
        separators=(",", ":"),
    ).encode()
    BUNDLE.parent.mkdir(parents=True, exist_ok=True)
    BUNDLE.write_bytes(zlib.compress(doc, 9))
    with_panel = sum(1 for m in models if m["panel"])
    print(
        f"{BUNDLE.relative_to(ROOT)}: {len(models)} models, {with_panel} with a panel, "
        f"{len(doc)} bytes -> {BUNDLE.stat().st_size} compressed"
    )


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument(
        "--fetch", action="store_true", help="download the catalogue and all panels first"
    )
    args = parser.parse_args()
    if args.fetch:
        fetch_panels(fetch_catalog())
    build_bundle()


if __name__ == "__main__":
    main()
