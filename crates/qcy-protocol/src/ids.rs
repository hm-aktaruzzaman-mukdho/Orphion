//! Identifiers of the TLV entries carried on the command/settings characteristics.
//!
//! Values come from `com.qcymall.qcylibrary.dataBean.DataBean.CMDID_*`. The app
//! uses Chinese pinyin for some of them; the original names are kept in the
//! docs so the decompiled sources stay searchable.

macro_rules! command_ids {
    ($( $(#[$doc:meta])* $name:ident = $value:literal ),* $(,)?) => {
        /// A TLV identifier. Unknown values are preserved in [`CommandId::Other`]
        /// so that new firmware features never break decoding.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub enum CommandId {
            $( $(#[$doc])* $name, )*
            /// Any identifier this crate does not know about yet.
            Other(u8),
        }

        impl From<u8> for CommandId {
            fn from(value: u8) -> Self {
                match value {
                    $( $value => CommandId::$name, )*
                    other => CommandId::Other(other),
                }
            }
        }

        impl From<CommandId> for u8 {
            fn from(id: CommandId) -> u8 {
                match id {
                    $( CommandId::$name => $value, )*
                    CommandId::Other(other) => other,
                }
            }
        }

        impl CommandId {
            /// Human readable name (the Rust variant name).
            pub fn name(self) -> String {
                match self {
                    $( CommandId::$name => stringify!($name).to_string(), )*
                    CommandId::Other(v) => format!("Other({v:#04x})"),
                }
            }
        }
    };
}

command_ids! {
    /// Restore user settings to their defaults (`CMDID_RESET_DEFAULT`).
    ResetDefault = 0x01,
    /// Forget all paired phones (`CMDID_CLEAR_PAIR`).
    ClearPairing = 0x02,
    /// Full factory reset (`CMDID_FACTORY_RESET`).
    FactoryReset = 0x03,
    /// Media control / playback state (`CMDID_MUSICACTION`).
    MusicAction = 0x04,
    /// LED flash (`CMDID_LIGHT`).
    Light = 0x05,
    /// In-ear detection, legacy boolean (`CMDID_RUER`, 入耳 = "in ear").
    InEar = 0x06,
    /// Legacy noise reduction strength (`CMDID_NOISE`).
    Noise = 0x07,
    /// Media volume: left, right, max (`CMDID_VOICE`).
    Volume = 0x08,
    /// Gaming / low-latency mode (`CMDID_DIYANSHI`, 低延时 = "low latency").
    GameMode = 0x09,
    /// Legacy monitor / transparency (`CMDID_JIANTING`, 监听 = "monitor").
    Monitor = 0x0A,
    /// Legacy noise mode selector (`CMDID_NOISEMODE`).
    NoiseMode = 0x0C,
    /// Factory test mode (`CMDID_TESTMODE`).
    TestMode = 0x0D,
    /// Sleep mode (disables touch controls) (`CMDID_SLEEPMODE`).
    SleepMode = 0x10,
    /// Ear-tip fit test (`CMDID_COMPACTNESS`).
    FitTest = 0x11,
    /// LED mode (`CMDID_LEDMODE`).
    LedMode = 0x12,
    /// Timed power off (sleep timer) (`CMDID_POWERMANAGER`).
    PowerOffTimer = 0x14,
    /// Left/right sound balance, 0..=100 (`CMDID_BALANCE`).
    Balance = 0x16,
    /// Noise control: mode, scene, level (`CMDID_ANCSETTING`).
    Anc = 0x17,
    /// Bluetooth name shown to phones (`CMDID_PAIRNAME`).
    DeviceName = 0x18,
    /// Voice prompt language code, e.g. `WQ00` (`CMDID_VOICENAME`).
    PromptLanguage = 0x19,
    /// Volume of prompt tones / beeps: level, max (`CMDID_TONEVOLUME`).
    PromptVolume = 0x1D,
    /// Camera shutter (`CMDID_TAKEPHOTO`).
    TakePhoto = 0x1E,
    /// aptX codec switch; the earbuds restart (no constant in the app).
    Aptx = 0x1B,
    /// Power off after being disconnected for N minutes (`CMDID_STANDBY`).
    AutoPowerOff = 0x1F,
    /// Parametric EQ, 6-byte bands (`CMDID_MULTIEQ`).
    MultiEq = 0x20,
    /// Parametric EQ, 7-byte bands with filter type (`CMDID_MULTIEQ2`).
    MultiEq2 = 0x22,
    /// LDAC codec switch (`CMDID_LDAC`).
    Ldac = 0x23,
    /// Multipoint / dual device connection (`CMDID_MOREDEVICE`).
    DualDevice = 0x24,
    /// Adaptive EQ (`CMDID_AEQ`).
    AdaptiveEq = 0x27,
    /// ANC result (`CMDID_ANC_RESULT`).
    AncResult = 0x28,
    /// ANC wear state (`CMDID_ANC_WEAR`).
    AncWear = 0x29,
    /// Anti-wind noise switch (no constant in the app).
    AntiWind = 0x2A,
    /// Touch key functions inside a frame (`CMDID_KEYFUN`).
    KeyFunctions = 0x2B,
    /// Wear detection configuration (`CMDID_PEIDAI`, 佩戴 = "wearing").
    WearDetection = 0x2C,
    /// Spatial audio (`CMDID_SPACE_AUDIO`).
    SpatialAudio = 0x2D,
    /// Music mode (`CMDID_MUSIC_MODE`).
    MusicMode = 0x2E,
    /// Battery inside a frame (`CMDID_BATTERY`).
    Battery = 0x2F,
    /// Version inside a frame (`CMDID_VERSION`).
    Version = 0x30,
    /// Environment adaptation (`CMDID_ENV_ADAPTATION`).
    EnvAdaptation = 0x32,
    /// Unnamed level setting offered as Medium/Normal/High (no constant in
    /// the app; takes effect after a restart).
    Level51 = 0x33,
    /// TWS enable (`CMDID_TWS_ENABLE`).
    TwsEnable = 0x34,
    /// LED switch (`CMDID_LED_SWITCH`).
    LedSwitch = 0x35,
    /// LED effect (`CMDID_LED_EFFECT`).
    LedEffect = 0x36,
    /// Playback source: 2 = SD card / focus, 4 = aux (`CMDID_PLAY_MODE`).
    PlayMode = 0x37,
    /// Bluetooth (0) or focus / SD card (1) mode (`CMDID_FOCUS_MODE`).
    FocusMode = 0x39,
    /// Synchronise the clock (`CMDID_SYNC_TIME`).
    SyncTime = 0x3E,
    /// Adaptive volume switch (no constant in the app).
    AdaptiveVolume = 0x40,
    /// Maximum number of parametric EQ bands supported (`CMDID_MAXEQ_COUNT`).
    MaxEqCount = 0x44,
    /// Parametric EQ of the left bud only (`CMDID_MULTIEQ_LEFT`).
    MultiEqLeft = 0x46,
    /// Parametric EQ of the right bud only (`CMDID_MULTIEQ_RIGHT`).
    MultiEqRight = 0x47,
    /// In-ear sensitivity (`CMDID_INEAR_SENSITIVITY`).
    InEarSensitivity = 0x48,
    /// Game mode with optional game EQ sync: `[enabled, sync]` (`CMDID_GAME_CONFIG`).
    GameConfig = 0x4A,
    /// Paired phones list (`CMDID_PHONE_INFO`).
    PhoneInfo = 0x4D,
    /// Paired phone status (`CMDID_PHONE_STATUS`).
    PhoneStatus = 0x4F,
    /// Head gestures (`CMDID_HEAD_ACTION`).
    HeadAction = 0x50,
    /// Head gesture configuration (`CMDID_SENSOR_CONFIG`).
    SensorConfig = 0x51,
    /// Eco mode: ANC off at low battery (no constant in the app).
    EcoMode = 0x53,
    /// AI ANC (`CMDID_AI_ANC`).
    AiAnc = 0x54,
    /// Bass enhancement switch (no constant in the app).
    BassBoost = 0x57,
    /// Hearing health statistics (`CMDID_HEAR_HEALTH`).
    HearHealth = 0x58,
    /// Spatial audio mode (`CMDID_SPACE_AUDIO_MODE`).
    SpatialAudioMode = 0x5B,
    /// "Please send me the current value of setting X" (`CMDID_REQUESTDATA`).
    RequestData = 0xFE,
}

#[cfg(test)]
mod tests {
    use super::CommandId;

    #[test]
    fn roundtrip() {
        for v in 0..=u8::MAX {
            assert_eq!(u8::from(CommandId::from(v)), v);
        }
        assert_eq!(CommandId::from(0x24), CommandId::DualDevice);
        assert_eq!(CommandId::from(0x99), CommandId::Other(0x99));
    }
}
