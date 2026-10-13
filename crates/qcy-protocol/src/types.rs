//! Strongly typed values carried by the protocol.

use std::fmt;

use crate::{CommandId, ProtocolError};

// ---------------------------------------------------------------------------
// Battery & version
// ---------------------------------------------------------------------------

/// Charge state of one component (left bud, right bud or case).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BatteryLevel {
    /// 1..=100 %.
    pub percent: u8,
    pub charging: bool,
}

/// Battery state of the whole set, from characteristic `0x0008`.
///
/// Each component is one byte: bit 7 = charging, bits 0..=6 = percent.
/// A percentage of 0 (or > 100) means "unknown / not connected".
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BatteryStatus {
    pub left: Option<BatteryLevel>,
    pub right: Option<BatteryLevel>,
    pub case: Option<BatteryLevel>,
}

impl BatteryStatus {
    pub fn parse(bytes: &[u8]) -> Result<Self, ProtocolError> {
        if bytes.len() < 3 {
            return Err(ProtocolError::TooShort {
                expected: 3,
                actual: bytes.len(),
            });
        }
        let cell = |b: u8| {
            let percent = b & 0x7F;
            (1..=100).contains(&percent).then_some(BatteryLevel {
                percent,
                charging: b & 0x80 != 0,
            })
        };
        Ok(Self {
            left: cell(bytes[0]),
            right: cell(bytes[1]),
            case: cell(bytes[2]),
        })
    }
}

/// A `major.minor.patch` firmware version.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version(pub u8, pub u8, pub u8);

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.0, self.1, self.2)
    }
}

/// Firmware of both buds, from characteristic `0x0007`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FirmwareVersions {
    pub left: Option<Version>,
    pub right: Option<Version>,
}

impl FirmwareVersions {
    pub fn parse(bytes: &[u8]) -> Self {
        let v = |b: &[u8]| Version(b[0], b[1], b[2]);
        Self {
            left: bytes.get(0..3).map(v),
            right: bytes.get(3..6).map(v),
        }
    }
}

// ---------------------------------------------------------------------------
// Boolean switches
// ---------------------------------------------------------------------------

/// Simple on/off settings. On the wire: `[id, 1, 1]` = on, `[id, 1, 2]` = off.
/// When reading, only the value `1` means "on".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Toggle {
    /// Low-latency gaming mode.
    GameMode,
    /// Sleep mode: touch controls disabled until the buds go back in the case.
    SleepMode,
    /// LDAC codec. The MeloBuds Pro reboot and disable dual-device when enabled.
    Ldac,
    /// Multipoint: be connected to two source devices at once.
    DualDevice,
    /// Legacy in-ear detection switch used by older models.
    InEarDetection,
}

impl Toggle {
    pub const ALL: [Toggle; 5] = [
        Toggle::GameMode,
        Toggle::SleepMode,
        Toggle::Ldac,
        Toggle::DualDevice,
        Toggle::InEarDetection,
    ];

    pub const fn command_id(self) -> CommandId {
        match self {
            Toggle::GameMode => CommandId::GameMode,
            Toggle::SleepMode => CommandId::SleepMode,
            Toggle::Ldac => CommandId::Ldac,
            Toggle::DualDevice => CommandId::DualDevice,
            Toggle::InEarDetection => CommandId::InEar,
        }
    }

    pub fn from_command_id(id: CommandId) -> Option<Self> {
        Self::ALL.into_iter().find(|t| t.command_id() == id)
    }

    pub const fn label(self) -> &'static str {
        match self {
            Toggle::GameMode => "Gaming mode",
            Toggle::SleepMode => "Sleep mode",
            Toggle::Ldac => "LDAC",
            Toggle::DualDevice => "Dual device connection",
            Toggle::InEarDetection => "In-ear detection",
        }
    }

    /// Wire value for a requested state.
    pub const fn encode(on: bool) -> u8 {
        if on { 1 } else { 2 }
    }
}

// ---------------------------------------------------------------------------
// Noise control (ANC / transparency)
// ---------------------------------------------------------------------------

/// Raw noise control setting as sent with [`CommandId::Anc`]:
/// `[0x17, 3, mode, scene, level]`.
///
/// The app packs it into one integer `mode << 16 | scene << 8 | level`
/// (the `startcmdid`/`endcmdid` values in the control panel JSON).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AncSetting {
    pub mode: u8,
    pub scene: u8,
    pub level: u8,
}

impl AncSetting {
    pub const fn new(mode: u8, scene: u8, level: u8) -> Self {
        Self { mode, scene, level }
    }

    /// Packed representation used by the QCY control panel JSON.
    pub const fn packed(self) -> u32 {
        (self.mode as u32) << 16 | (self.scene as u32) << 8 | self.level as u32
    }

    pub const fn from_packed(v: u32) -> Self {
        Self::new((v >> 16) as u8, (v >> 8) as u8, v as u8)
    }
}

/// Noise cancelling scenes offered by current QCY firmwares (MeloBuds Pro).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AncScene {
    /// Quiet environments (levels 0..=2).
    Indoor,
    /// Very noisy environments such as commuting (levels 0..=2).
    Commuting,
    /// Noisy environments (levels 0..=2).
    Noisy,
    /// Reduces wind noise (no levels).
    AntiWind,
    /// The earbuds pick the strength themselves (no levels).
    Adaptive,
}

impl AncScene {
    pub const ALL: [AncScene; 5] = [
        AncScene::Adaptive,
        AncScene::Indoor,
        AncScene::Commuting,
        AncScene::Noisy,
        AncScene::AntiWind,
    ];

    pub const fn id(self) -> u8 {
        match self {
            AncScene::Indoor => 1,
            AncScene::Commuting => 2,
            AncScene::Noisy => 3,
            AncScene::AntiWind => 4,
            AncScene::Adaptive => 5,
        }
    }

    pub fn from_id(id: u8) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.id() == id)
    }

    /// Highest strength level for this scene (0 = no level selection).
    pub const fn max_level(self) -> u8 {
        match self {
            AncScene::Indoor | AncScene::Commuting | AncScene::Noisy => 2,
            AncScene::AntiWind | AncScene::Adaptive => 0,
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            AncScene::Indoor => "Indoor",
            AncScene::Commuting => "Commuting",
            AncScene::Noisy => "Noisy",
            AncScene::AntiWind => "Anti-wind",
            AncScene::Adaptive => "Adaptive",
        }
    }
}

/// High level view of the noise control state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AncMode {
    /// Neither ANC nor transparency ("Normal" in the app). Wire: `(2, 0, 0)`.
    Off,
    /// Active noise cancelling. Wire: `(1, scene, level)`.
    NoiseCancelling { scene: AncScene, level: u8 },
    /// Ambient sound pass-through. Wire: `(3, 1, level)`.
    Transparency { level: u8 },
    /// Transparency tuned for voices ("vocal" checkbox in the app). Wire: `(3, 2, 0)`.
    VoiceTransparency,
    /// Anything this crate cannot interpret; preserved verbatim.
    Other(AncSetting),
}

impl AncMode {
    pub const MODE_ANC: u8 = 1;
    pub const MODE_OFF: u8 = 2;
    pub const MODE_TRANSPARENCY: u8 = 3;

    pub fn to_setting(self) -> AncSetting {
        match self {
            AncMode::Off => AncSetting::new(Self::MODE_OFF, 0, 0),
            AncMode::NoiseCancelling { scene, level } => {
                AncSetting::new(Self::MODE_ANC, scene.id(), level)
            }
            AncMode::Transparency { level } => AncSetting::new(Self::MODE_TRANSPARENCY, 1, level),
            AncMode::VoiceTransparency => AncSetting::new(Self::MODE_TRANSPARENCY, 2, 0),
            AncMode::Other(s) => s,
        }
    }

    pub fn from_setting(s: AncSetting) -> Self {
        match (s.mode, s.scene) {
            (Self::MODE_OFF, _) => AncMode::Off,
            (Self::MODE_ANC, scene) => match AncScene::from_id(scene) {
                Some(scene) => AncMode::NoiseCancelling {
                    scene,
                    level: s.level,
                },
                None => AncMode::Other(s),
            },
            (Self::MODE_TRANSPARENCY, 1) => AncMode::Transparency { level: s.level },
            (Self::MODE_TRANSPARENCY, 2) => AncMode::VoiceTransparency,
            _ => AncMode::Other(s),
        }
    }

    /// Short description, e.g. `"ANC · Indoor 2"`.
    pub fn describe(self) -> String {
        match self {
            AncMode::Off => "Normal".into(),
            AncMode::NoiseCancelling { scene, level } if scene.max_level() > 0 => {
                format!("Noise cancelling · {} {}", scene.label(), level)
            }
            AncMode::NoiseCancelling { scene, .. } => {
                format!("Noise cancelling · {}", scene.label())
            }
            AncMode::Transparency { level } => format!("Transparency · level {level}"),
            AncMode::VoiceTransparency => "Transparency · voice".into(),
            AncMode::Other(s) => format!("Unknown ({:#08x})", s.packed()),
        }
    }
}

// ---------------------------------------------------------------------------
// Touch controls
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Side {
    Left,
    Right,
}

impl Side {
    pub const fn label(self) -> &'static str {
        match self {
            Side::Left => "Left",
            Side::Right => "Right",
        }
    }
}

/// Kind of touch gesture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Tap {
    Single,
    Double,
    Triple,
    Quadruple,
    Long,
}

impl Tap {
    pub const fn label(self) -> &'static str {
        match self {
            Tap::Single => "Single tap",
            Tap::Double => "Double tap",
            Tap::Triple => "Triple tap",
            Tap::Quadruple => "Quadruple tap",
            Tap::Long => "Long press",
        }
    }
}

/// A gesture slot of the key table (characteristic `0x000D`).
///
/// IDs from `QCYHeadsetClient.KEYID_MUSIC_*`: odd = left, even = right;
/// 1/2 single, 3/4 double, 5/6 triple, 7/8 quadruple, 9/10 long press.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Gesture {
    pub side: Side,
    pub tap: Tap,
}

impl Gesture {
    pub const fn new(side: Side, tap: Tap) -> Self {
        Self { side, tap }
    }

    pub const fn id(self) -> u8 {
        let base = match self.tap {
            Tap::Single => 1,
            Tap::Double => 3,
            Tap::Triple => 5,
            Tap::Quadruple => 7,
            Tap::Long => 9,
        };
        match self.side {
            Side::Left => base,
            Side::Right => base + 1,
        }
    }

    pub const fn from_id(id: u8) -> Option<Self> {
        let tap = match id {
            1 | 2 => Tap::Single,
            3 | 4 => Tap::Double,
            5 | 6 => Tap::Triple,
            7 | 8 => Tap::Quadruple,
            9 | 10 => Tap::Long,
            _ => return None,
        };
        let side = if id % 2 == 1 { Side::Left } else { Side::Right };
        Some(Self { side, tap })
    }

    pub fn label(self) -> String {
        format!("{} · {}", self.side.label(), self.tap.label())
    }
}

/// Action bound to a gesture.
///
/// Values 0..=7 are shared by every control panel definition the app ships;
/// 11 ("ANC mode", cycles noise control) comes from the MeloBuds Pro panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyAction {
    None,
    PlayPause,
    PreviousTrack,
    NextTrack,
    VoiceAssistant,
    VolumeUp,
    VolumeDown,
    GameMode,
    CycleAnc,
    Other(u8),
}

impl KeyAction {
    /// Actions offered for the MeloBuds Pro, in the app's order.
    pub const KNOWN: [KeyAction; 9] = [
        KeyAction::None,
        KeyAction::PlayPause,
        KeyAction::PreviousTrack,
        KeyAction::NextTrack,
        KeyAction::VoiceAssistant,
        KeyAction::VolumeUp,
        KeyAction::VolumeDown,
        KeyAction::GameMode,
        KeyAction::CycleAnc,
    ];

    pub const fn id(self) -> u8 {
        match self {
            KeyAction::None => 0,
            KeyAction::PlayPause => 1,
            KeyAction::PreviousTrack => 2,
            KeyAction::NextTrack => 3,
            KeyAction::VoiceAssistant => 4,
            KeyAction::VolumeUp => 5,
            KeyAction::VolumeDown => 6,
            KeyAction::GameMode => 7,
            KeyAction::CycleAnc => 11,
            KeyAction::Other(v) => v,
        }
    }

    pub fn from_id(id: u8) -> Self {
        Self::KNOWN
            .into_iter()
            .find(|a| a.id() == id)
            .unwrap_or(KeyAction::Other(id))
    }

    pub fn label(self) -> String {
        match self {
            KeyAction::None => "No action".into(),
            KeyAction::PlayPause => "Play / pause".into(),
            KeyAction::PreviousTrack => "Previous track".into(),
            KeyAction::NextTrack => "Next track".into(),
            KeyAction::VoiceAssistant => "Voice assistant".into(),
            KeyAction::VolumeUp => "Volume up".into(),
            KeyAction::VolumeDown => "Volume down".into(),
            KeyAction::GameMode => "Gaming mode".into(),
            KeyAction::CycleAnc => "Cycle noise control".into(),
            KeyAction::Other(v) => format!("Unknown ({v})"),
        }
    }
}

/// One `(gesture id, action id)` pair of the key table, kept raw so unknown
/// slots survive a read-modify-write cycle untouched.
///
/// Gesture ids are the panel's event ids: 1..=10 for touch gestures, 17..32
/// for headphone buttons, and `key << 8 | clicks` for JieLi headphone keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyBinding {
    pub gesture_id: u16,
    pub action: KeyAction,
}

impl KeyBinding {
    pub fn gesture(&self) -> Option<Gesture> {
        u8::try_from(self.gesture_id)
            .ok()
            .and_then(Gesture::from_id)
    }

    /// Parses the key table: a flat list of `[gesture, action]` byte pairs.
    /// Zero gesture ids are padding and skipped.
    pub fn parse_table(bytes: &[u8]) -> Vec<KeyBinding> {
        bytes
            .chunks_exact(2)
            .filter(|pair| pair[0] != 0)
            .map(|pair| KeyBinding {
                gesture_id: u16::from(pair[0]),
                action: KeyAction::from_id(pair[1]),
            })
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Wear detection & power management
// ---------------------------------------------------------------------------

/// What happens to playback when the buds are taken out / put back in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WearMusicBehavior {
    /// Do nothing.
    Off,
    /// Pause when taken off, resume when put on.
    PauseAndResume,
    /// Pause when taken off, do not resume.
    PauseOnly,
    /// Pause when one is taken off, resume only when both are worn.
    PauseAndResumeBoth,
    Other(u8),
}

impl WearMusicBehavior {
    pub const KNOWN: [WearMusicBehavior; 4] = [
        WearMusicBehavior::Off,
        WearMusicBehavior::PauseAndResume,
        WearMusicBehavior::PauseOnly,
        WearMusicBehavior::PauseAndResumeBoth,
    ];

    pub const fn id(self) -> u8 {
        match self {
            WearMusicBehavior::Off => 0,
            WearMusicBehavior::PauseAndResume => 1,
            WearMusicBehavior::PauseOnly => 2,
            WearMusicBehavior::PauseAndResumeBoth => 3,
            WearMusicBehavior::Other(v) => v,
        }
    }

    pub fn from_id(id: u8) -> Self {
        Self::KNOWN
            .into_iter()
            .find(|b| b.id() == id)
            .unwrap_or(WearMusicBehavior::Other(id))
    }

    pub fn label(self) -> String {
        match self {
            WearMusicBehavior::Off => "Do nothing".into(),
            WearMusicBehavior::PauseAndResume => "Pause and resume".into(),
            WearMusicBehavior::PauseOnly => "Pause only".into(),
            WearMusicBehavior::PauseAndResumeBoth => "Resume when both worn".into(),
            WearMusicBehavior::Other(v) => format!("Unknown ({v})"),
        }
    }
}

/// Wear detection configuration ([`CommandId::WearDetection`]).
///
/// Read: `[enabled, music, anc, tone?]`. Written with only the first three
/// bytes, exactly like `QCYConnectManager.setPeidaiConfig`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WearDetection {
    pub enabled: bool,
    pub music: WearMusicBehavior,
    /// Turn ANC off when removed and back on when worn.
    pub anc_follows_wear: bool,
    /// Prompt tone on wear (read only, newer firmwares).
    pub tone: Option<bool>,
}

impl WearDetection {
    pub fn parse(p: &[u8]) -> Result<Self, ProtocolError> {
        if p.len() < 3 {
            return Err(ProtocolError::TooShort {
                expected: 3,
                actual: p.len(),
            });
        }
        Ok(Self {
            enabled: p[0] == 1,
            music: WearMusicBehavior::from_id(p[1]),
            anc_follows_wear: p[2] == 1,
            tone: p.get(3).map(|&b| b == 1),
        })
    }

    pub fn encode(&self) -> [u8; 3] {
        [
            u8::from(self.enabled),
            self.music.id(),
            u8::from(self.anc_follows_wear),
        ]
    }
}

/// Power off after being disconnected from every phone for a while
/// ([`CommandId::AutoPowerOff`], u16 LE minutes, `0xFFFF` = never).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AutoPowerOff {
    Never,
    After { minutes: u16 },
}

impl AutoPowerOff {
    const NEVER: u16 = 0xFFFF;

    /// Choices offered by the app for the MeloBuds Pro.
    pub const PRESETS: [AutoPowerOff; 5] = [
        AutoPowerOff::After { minutes: 5 },
        AutoPowerOff::After { minutes: 10 },
        AutoPowerOff::After { minutes: 30 },
        AutoPowerOff::After { minutes: 60 },
        AutoPowerOff::Never,
    ];

    pub fn parse(p: &[u8]) -> Result<Self, ProtocolError> {
        let raw = read_u16_le(p)?;
        Ok(if raw == Self::NEVER {
            AutoPowerOff::Never
        } else {
            AutoPowerOff::After { minutes: raw }
        })
    }

    pub fn encode(self) -> [u8; 2] {
        match self {
            AutoPowerOff::Never => Self::NEVER.to_le_bytes(),
            AutoPowerOff::After { minutes } => minutes.min(Self::NEVER - 1).to_le_bytes(),
        }
    }

    pub fn label(self) -> String {
        match self {
            AutoPowerOff::Never => "Never".into(),
            AutoPowerOff::After { minutes } => format!("{minutes} min"),
        }
    }
}

/// Sleep timer ([`CommandId::PowerOffTimer`]): power off after N minutes even
/// while in use. Written as `[min_lo, min_hi, 0, 0]`; 0 cancels the timer.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PowerOffTimer {
    /// Configured duration in minutes (0 = disabled).
    pub minutes: u16,
    /// Value of the second field reported by the device (elapsed/remaining,
    /// semantics not confirmed).
    pub progress: u16,
}

impl PowerOffTimer {
    pub fn parse(p: &[u8]) -> Result<Self, ProtocolError> {
        if p.len() < 4 {
            return Err(ProtocolError::TooShort {
                expected: 4,
                actual: p.len(),
            });
        }
        Ok(Self {
            minutes: u16::from_le_bytes([p[0], p[1]]),
            progress: u16::from_le_bytes([p[2], p[3]]),
        })
    }
}

/// Media volume as reported by [`CommandId::Volume`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Volume {
    pub left: u8,
    pub right: u8,
    pub max: u8,
}

/// Prompt ("beep") volume as reported by [`CommandId::PromptVolume`]
/// (wire id `CMDID_TONEVOLUME`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PromptVolume {
    pub level: u8,
    pub max: u8,
}

pub(crate) fn read_u16_le(p: &[u8]) -> Result<u16, ProtocolError> {
    match p {
        [lo, hi, ..] => Ok(u16::from_le_bytes([*lo, *hi])),
        _ => Err(ProtocolError::TooShort {
            expected: 2,
            actual: p.len(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn battery_real_sample() {
        // 0x0008 read from a MeloBuds Pro: 65 %, 75 %, case unknown.
        let b = BatteryStatus::parse(&[0x41, 0x4b, 0x00]).unwrap();
        assert_eq!(b.left.unwrap().percent, 65);
        assert_eq!(b.right.unwrap().percent, 75);
        assert!(b.case.is_none());
        let charging = BatteryStatus::parse(&[0x80 | 50, 0, 0]).unwrap();
        assert!(charging.left.unwrap().charging);
    }

    #[test]
    fn version_real_sample() {
        let v = FirmwareVersions::parse(&[2, 0, 6, 2, 0, 6]);
        assert_eq!(v.left.unwrap().to_string(), "2.0.6");
        assert_eq!(v.right, v.left);
    }

    #[test]
    fn anc_roundtrip() {
        for mode in [
            AncMode::Off,
            AncMode::NoiseCancelling {
                scene: AncScene::Adaptive,
                level: 0,
            },
            AncMode::NoiseCancelling {
                scene: AncScene::Commuting,
                level: 2,
            },
            AncMode::Transparency { level: 4 },
            AncMode::VoiceTransparency,
        ] {
            assert_eq!(AncMode::from_setting(mode.to_setting()), mode);
        }
        // Panel JSON: default ANC = 65794 = 0x010102 = indoor, level 2.
        assert_eq!(
            AncMode::from_setting(AncSetting::from_packed(65794)),
            AncMode::NoiseCancelling {
                scene: AncScene::Indoor,
                level: 2
            }
        );
        assert_eq!(AncSetting::from_packed(131072), AncMode::Off.to_setting());
    }

    #[test]
    fn gestures() {
        for id in 1..=10 {
            assert_eq!(Gesture::from_id(id).unwrap().id(), id);
        }
        assert_eq!(
            Gesture::from_id(5),
            Some(Gesture::new(Side::Left, Tap::Triple))
        );
        assert!(Gesture::from_id(0).is_none());
    }

    #[test]
    fn key_table_real_sample() {
        let raw = [1, 0, 2, 0, 3, 1, 4, 1, 5, 4, 6, 3, 7, 0, 8, 0, 0, 0, 0, 0];
        let table = KeyBinding::parse_table(&raw);
        assert_eq!(table.len(), 8);
        assert_eq!(table[4].action, KeyAction::VoiceAssistant);
        assert_eq!(table[5].action, KeyAction::NextTrack);
    }

    #[test]
    fn wear_and_power() {
        let w = WearDetection::parse(&[0, 1, 1, 1]).unwrap();
        assert!(!w.enabled);
        assert_eq!(w.music, WearMusicBehavior::PauseAndResume);
        assert_eq!(w.encode(), [0, 1, 1]);

        assert_eq!(
            AutoPowerOff::parse(&[5, 0]).unwrap(),
            AutoPowerOff::After { minutes: 5 }
        );
        assert_eq!(
            AutoPowerOff::parse(&[0xff, 0xff]).unwrap(),
            AutoPowerOff::Never
        );
        assert_eq!(AutoPowerOff::Never.encode(), [0xff, 0xff]);
    }
}
