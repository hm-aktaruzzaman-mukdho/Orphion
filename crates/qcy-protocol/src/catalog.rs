//! The bundled model catalogue: every QCY earphone and its control panel.
//!
//! The official app downloads a "control panel" JSON per model id from
//! `https://api.watch.qcy.com/product/findControlPanels` and builds its whole
//! UI from it. A copy of all of them (193 models) is compiled into this crate,
//! so the desktop app works offline, without a QCY account, and knows every
//! model's full feature set. `scripts/update-panels.py --fetch` refreshes the
//! copy.
//!
//! Like the app, models without a panel fall back to a built-in default:
//! one for JieLi chips and one for everything else.

use std::collections::HashMap;
use std::sync::LazyLock;

use serde::Deserialize;
use serde_json::value::RawValue;

use crate::advert::ChipFamily;
use crate::panel::Panel;

/// zlib-compressed bundle written by `scripts/update-panels.py`.
const BUNDLE: &[u8] = include_bytes!("../data/panels.json.zz");

#[derive(Deserialize)]
struct Bundle {
    defaults: Defaults,
    models: Vec<Entry>,
}

#[derive(Deserialize)]
struct Defaults {
    qcy: Box<RawValue>,
    jieli: Box<RawValue>,
}

#[derive(Deserialize)]
struct Entry {
    id: u16,
    name: String,
    panel: Option<Box<RawValue>>,
}

/// A model of the catalogue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Model {
    /// Model id, as advertised (`vendorID` in QCY's API).
    pub id: u16,
    /// Marketing name, e.g. "QCY MeloBuds Pro".
    pub name: String,
    pub chip: ChipFamily,
    /// Whether QCY publishes a panel for it (otherwise the default applies).
    pub has_panel: bool,
}

struct Catalog {
    models: Vec<Model>,
    panels: HashMap<u16, Box<RawValue>>,
    defaults: Defaults,
}

static CATALOG: LazyLock<Catalog> = LazyLock::new(|| {
    let json = miniz_oxide::inflate::decompress_to_vec_zlib(BUNDLE)
        .expect("the bundled panel catalogue is valid zlib");
    let bundle: Bundle =
        serde_json::from_slice(&json).expect("the bundled panel catalogue is valid JSON");
    let mut models = Vec::with_capacity(bundle.models.len());
    let mut panels = HashMap::new();
    for e in bundle.models {
        models.push(Model {
            id: e.id,
            name: e.name,
            chip: ChipFamily::from_vendor_id(e.id),
            has_panel: e.panel.is_some(),
        });
        if let Some(p) = e.panel {
            panels.insert(e.id, p);
        }
    }
    Catalog {
        models,
        panels,
        defaults: bundle.defaults,
    }
});

/// Every model of the catalogue, ordered by id.
pub fn models() -> &'static [Model] {
    &CATALOG.models
}

/// Looks up a model by id.
pub fn model(id: u16) -> Option<&'static Model> {
    CATALOG.models.iter().find(|m| m.id == id)
}

/// Marketing name of a model, if it is in the catalogue.
pub fn name(id: u16) -> Option<&'static str> {
    model(id).map(|m| m.name.as_str())
}

/// The control panel of a model: its own, or the app's default for its chip
/// family when QCY publishes none (or the model is unknown).
pub fn panel(id: u16) -> Panel {
    let c = &*CATALOG;
    let raw = c.panels.get(&id).unwrap_or_else(|| {
        if ChipFamily::from_vendor_id(id) == ChipFamily::JieLi {
            &c.defaults.jieli
        } else {
            &c.defaults.qcy
        }
    });
    Panel::parse_str(raw.get()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundle_loads() {
        assert!(models().len() > 150);
        assert_eq!(name(19786), Some("QCY MeloBuds Pro"));
        assert_eq!(model(19786).unwrap().chip, ChipFamily::WuQi);
    }

    #[test]
    fn every_panel_parses() {
        for m in models() {
            let raw = CATALOG.panels.get(&m.id);
            if let Some(raw) = raw {
                Panel::parse_str(raw.get())
                    .unwrap_or_else(|e| panic!("panel of {} ({}): {e}", m.name, m.id));
            }
        }
        Panel::parse_str(CATALOG.defaults.qcy.get()).unwrap();
        Panel::parse_str(CATALOG.defaults.jieli.get()).unwrap();
    }

    #[test]
    fn unknown_models_get_a_default() {
        assert!(panel(1).touch_keys.is_some());
        assert!(panel(0x8071).eq.is_some());
    }
}
