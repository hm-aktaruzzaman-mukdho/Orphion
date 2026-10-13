//! Command line definition.
//!
//! Model specific names (modes, settings, gestures, actions) come from the
//! connected model's control panel; run a command without its value to list
//! what the model accepts, e.g. `qcyctl anc` or `qcyctl set`.

use clap::{Parser, Subcommand, ValueEnum};
use qcy_protocol::advert::MacAddress;
use qcy_protocol::types::WearMusicBehavior;

#[derive(Debug, Parser)]
#[command(
    name = "qcyctl",
    version,
    about = "Control QCY earbuds from Linux or Windows"
)]
pub struct Cli {
    /// Earbuds to use, by classic (audio) or BLE address. Default: auto-detect.
    #[arg(short, long, global = true, value_name = "MAC")]
    pub device: Option<MacAddress>,

    /// Seconds to scan for the earbuds.
    #[arg(long, global = true, default_value_t = 12)]
    pub timeout: u64,

    /// Keep the BLE control link open when done (faster next time, but the
    /// phone app cannot connect while it is open).
    #[arg(long, global = true)]
    pub keep_connected: bool,

    /// More logging (-v: debug, -vv: protocol trace).
    #[arg(short, long, global = true, action = clap::ArgAction::Count)]
    pub verbose: u8,

    #[command(subcommand)]
    pub command: Option<Cmd>,
}

#[derive(Debug, Subcommand)]
pub enum Cmd {
    /// List QCY earbuds nearby.
    Scan,
    /// List every model this program knows (no Bluetooth needed).
    Models {
        /// Only models whose name contains this text.
        filter: Option<String>,
    },
    /// Show the controls of a model, by id (no Bluetooth needed), or of the
    /// connected earbuds.
    Panel { model: Option<u16> },
    /// Show battery, firmware and every setting (default).
    Status,
    /// Print changes live until Ctrl+C.
    Monitor,
    /// Noise control: `anc` lists the modes, `anc MODE [SCENE] [--level N]` sets one.
    Anc {
        mode: Option<String>,
        /// Scene within the mode, or `voice` for voice focus.
        scene: Option<String>,
        /// Strength, counting from 1 (percent for percent-based models).
        #[arg(long)]
        level: Option<u32>,
    },
    /// Equalizer.
    Eq {
        #[command(subcommand)]
        action: Option<EqCmd>,
    },
    /// Change a setting: `set` lists them, `set NAME VALUE` changes one,
    /// e.g. `set ldac-switch on` or `set sleep-timer 30`.
    Set {
        name: Option<String>,
        value: Option<String>,
        /// Confirm settings that restart the earbuds.
        #[arg(long)]
        yes: bool,
    },
    /// Left/right balance: 0 = left, 50 = centre, 100 = right.
    Balance {
        #[arg(value_parser = clap::value_parser!(u8).range(0..=100))]
        value: u8,
    },
    /// Change the Bluetooth name (both buds out of the case).
    Rename { name: String },
    /// Wear detection details (the switch alone: `set wear-detection on`).
    WearDetection {
        #[arg(value_enum)]
        state: OnOff,
        /// What to do with playback.
        #[arg(long, value_enum)]
        music: Option<WearMusicArg>,
        /// Switch noise control off when removed and back on when worn.
        #[arg(long, value_enum)]
        anc: Option<OnOff>,
    },
    /// Touch, button and head gestures.
    Keys {
        #[command(subcommand)]
        action: Option<KeysCmd>,
    },
    /// Reset settings, pairings or everything (what the model offers).
    Reset {
        #[arg(value_enum)]
        what: ResetArg,
        /// Required confirmation.
        #[arg(long)]
        yes: bool,
    },
    /// Send raw hex bytes (for protocol research), e.g. `ff03fe0122`.
    Raw {
        hex: String,
        #[arg(long, value_enum, default_value_t = CharArg::Command)]
        characteristic: CharArg,
    },
    /// Close the BLE control link.
    Disconnect,
}

#[derive(Debug, Subcommand)]
pub enum EqCmd {
    /// Show the active EQ (default).
    Show,
    /// List the presets of this model.
    List,
    /// Apply a preset by name or id.
    Preset {
        name: String,
        /// Confirm presets that come with a warning.
        #[arg(long)]
        yes: bool,
    },
    /// Apply a game EQ preset (models with a game EQ).
    Game { name: String },
    /// Apply a custom curve: one gain in dB per band of the model.
    Custom {
        #[arg(num_args = 1.., allow_hyphen_values = true)]
        gains: Vec<f32>,
    },
}

#[derive(Debug, Subcommand)]
pub enum KeysCmd {
    /// Show the gestures and their actions (default).
    Show,
    /// Bind a gesture, e.g. `keys set left-double next`.
    Set { gesture: String, action: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum OnOff {
    On,
    Off,
}

impl OnOff {
    pub fn is_on(self) -> bool {
        self == OnOff::On
    }
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum WearMusicArg {
    Nothing,
    PauseResume,
    PauseOnly,
    PauseResumeBoth,
}

impl From<WearMusicArg> for WearMusicBehavior {
    fn from(w: WearMusicArg) -> Self {
        match w {
            WearMusicArg::Nothing => WearMusicBehavior::Off,
            WearMusicArg::PauseResume => WearMusicBehavior::PauseAndResume,
            WearMusicArg::PauseOnly => WearMusicBehavior::PauseOnly,
            WearMusicArg::PauseResumeBoth => WearMusicBehavior::PauseAndResumeBoth,
        }
    }
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum ResetArg {
    /// Restore settings to defaults, keep pairings.
    Defaults,
    /// Forget all paired phones.
    Pairing,
    /// Full factory reset.
    Factory,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum CharArg {
    Command,
    Keys,
    Eq,
}
