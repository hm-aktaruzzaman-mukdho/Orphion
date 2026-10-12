//! Command line definition.

use clap::{Parser, Subcommand, ValueEnum};
use qcy_protocol::advert::MacAddress;
use qcy_protocol::types::{AncScene, Gesture, KeyAction, Side, Tap, Toggle, WearMusicBehavior};

#[derive(Debug, Parser)]
#[command(name = "qcyctl", version, about = "Control QCY earbuds from Linux")]
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
    /// Show battery, firmware and every setting (default).
    Status,
    /// Print changes live until Ctrl+C.
    Monitor,
    /// Noise control.
    Anc {
        #[arg(value_enum)]
        mode: AncArg,
        /// Noise cancelling scene (with `on`).
        #[arg(long, value_enum, default_value_t = SceneArg::Adaptive)]
        scene: SceneArg,
        /// Strength: 0-2 for noise cancelling scenes, 1-6 for transparency.
        #[arg(long)]
        level: Option<u8>,
    },
    /// Equalizer.
    Eq {
        #[command(subcommand)]
        action: EqCmd,
    },
    /// Turn a feature on or off.
    Toggle {
        #[arg(value_enum)]
        feature: ToggleArg,
        #[arg(value_enum)]
        state: OnOff,
    },
    /// Left/right balance: 0 = left, 50 = centre, 100 = right.
    Balance {
        #[arg(value_parser = clap::value_parser!(u8).range(0..=100))]
        value: u8,
    },
    /// Volume of voice prompts and beeps (e.g. 4, 6, 8, 10).
    PromptVolume { level: u8 },
    /// Voice prompt language: `english`, `chinese` or a raw code like `WQ00`.
    Language { code: String },
    /// Change the Bluetooth name (both buds out of the case).
    Rename { name: String },
    /// Wear detection.
    WearDetection {
        #[arg(value_enum)]
        state: OnOff,
        /// What to do with playback.
        #[arg(long, value_enum)]
        music: Option<WearMusicArg>,
        /// Switch ANC off when removed and back on when worn.
        #[arg(long, value_enum)]
        anc: Option<OnOff>,
    },
    /// Power off after being disconnected for N minutes (`never` to disable).
    AutoOff { minutes: String },
    /// Power off after N minutes regardless of use (0 cancels).
    SleepTimer { minutes: u16 },
    /// Touch controls.
    Keys {
        #[command(subcommand)]
        action: Option<KeysCmd>,
    },
    /// Reset settings, pairings or everything.
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
    /// Show the active EQ.
    Show,
    /// List the presets of this model.
    List,
    /// Apply a preset by name or id.
    Preset { name: String },
    /// Apply a 10-band curve (31 Hz … 16 kHz), gains in dB from -8 to 8.
    Custom {
        #[arg(num_args = 10, allow_hyphen_values = true)]
        gains: Vec<i8>,
    },
}

#[derive(Debug, Subcommand)]
pub enum KeysCmd {
    /// Show the gesture table.
    Show,
    /// Bind a gesture, e.g. `keys set left-double next`.
    Set {
        #[arg(value_enum)]
        gesture: GestureArg,
        #[arg(value_enum)]
        action: ActionArg,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum AncArg {
    Off,
    On,
    Transparency,
    Voice,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum SceneArg {
    Adaptive,
    Indoor,
    Commuting,
    Noisy,
    AntiWind,
}

impl From<SceneArg> for AncScene {
    fn from(s: SceneArg) -> Self {
        match s {
            SceneArg::Adaptive => AncScene::Adaptive,
            SceneArg::Indoor => AncScene::Indoor,
            SceneArg::Commuting => AncScene::Commuting,
            SceneArg::Noisy => AncScene::Noisy,
            SceneArg::AntiWind => AncScene::AntiWind,
        }
    }
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum ToggleArg {
    GameMode,
    SleepMode,
    Ldac,
    DualDevice,
}

impl From<ToggleArg> for Toggle {
    fn from(t: ToggleArg) -> Self {
        match t {
            ToggleArg::GameMode => Toggle::GameMode,
            ToggleArg::SleepMode => Toggle::SleepMode,
            ToggleArg::Ldac => Toggle::Ldac,
            ToggleArg::DualDevice => Toggle::DualDevice,
        }
    }
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
pub enum GestureArg {
    LeftSingle,
    RightSingle,
    LeftDouble,
    RightDouble,
    LeftTriple,
    RightTriple,
    LeftQuadruple,
    RightQuadruple,
    LeftLong,
    RightLong,
}

impl From<GestureArg> for Gesture {
    fn from(g: GestureArg) -> Self {
        use GestureArg::*;
        let (side, tap) = match g {
            LeftSingle => (Side::Left, Tap::Single),
            RightSingle => (Side::Right, Tap::Single),
            LeftDouble => (Side::Left, Tap::Double),
            RightDouble => (Side::Right, Tap::Double),
            LeftTriple => (Side::Left, Tap::Triple),
            RightTriple => (Side::Right, Tap::Triple),
            LeftQuadruple => (Side::Left, Tap::Quadruple),
            RightQuadruple => (Side::Right, Tap::Quadruple),
            LeftLong => (Side::Left, Tap::Long),
            RightLong => (Side::Right, Tap::Long),
        };
        Gesture::new(side, tap)
    }
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum ActionArg {
    None,
    PlayPause,
    Previous,
    Next,
    Assistant,
    VolumeUp,
    VolumeDown,
    GameMode,
    Anc,
}

impl From<ActionArg> for KeyAction {
    fn from(a: ActionArg) -> Self {
        match a {
            ActionArg::None => KeyAction::None,
            ActionArg::PlayPause => KeyAction::PlayPause,
            ActionArg::Previous => KeyAction::PreviousTrack,
            ActionArg::Next => KeyAction::NextTrack,
            ActionArg::Assistant => KeyAction::VoiceAssistant,
            ActionArg::VolumeUp => KeyAction::VolumeUp,
            ActionArg::VolumeDown => KeyAction::VolumeDown,
            ActionArg::GameMode => KeyAction::GameMode,
            ActionArg::Anc => KeyAction::CycleAnc,
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
