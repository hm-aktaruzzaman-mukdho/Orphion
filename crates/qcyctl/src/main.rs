//! `qcyctl` – command line control for QCY earbuds.
//!
//! Every invocation finds the earbuds, opens the BLE control link, performs
//! one action and (unless `--keep-connected`) closes the link again.

mod cli;
mod print;

use std::time::Duration;

use anyhow::{Context, Result, bail};
use clap::Parser;
use qcy_bluez::discovery::Selector;
use qcy_bluez::{Bluez, Earbuds};
use qcy_protocol::eq::ParametricEq;
use qcy_protocol::types::{AncMode, AutoPowerOff, Gesture, WearDetection, WearMusicBehavior};
use qcy_protocol::{Characteristic, Command, Write};
use tracing_subscriber::EnvFilter;

use crate::cli::{AncArg, CharArg, Cli, Cmd, EqCmd, KeysCmd, ResetArg};

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    init_logging(cli.verbose);

    let bluez = Bluez::connect().await.context("cannot reach BlueZ")?;
    let timeout = Duration::from_secs(cli.timeout);

    if matches!(cli.command, Some(Cmd::Scan)) {
        println!("Scanning for {}s…", cli.timeout.min(6));
        let found = bluez.scan(timeout.min(Duration::from_secs(6))).await?;
        if found.is_empty() {
            println!("No QCY earbuds found. Are they out of the case?");
        }
        found.iter().for_each(print::endpoint);
        return Ok(());
    }

    let selector = cli.device.map_or(Selector::Auto, Selector::Address);
    let endpoint = bluez.find(&selector, timeout).await?;
    let buds = Earbuds::connect(&bluez, &endpoint).await?;
    // Give replies to the initial requests a moment to arrive.
    tokio::time::sleep(Duration::from_millis(400)).await;

    let result = run(&buds, cli.command.unwrap_or(Cmd::Status)).await;

    if !cli.keep_connected {
        buds.disconnect().await.ok();
    }
    result
}

async fn run(buds: &Earbuds, cmd: Cmd) -> Result<()> {
    let profile = buds.profile();
    let name = buds.endpoint().display_name();

    match cmd {
        Cmd::Scan => unreachable!("handled before connecting"),
        Cmd::Status => print::status(&name, profile, &buds.state()),
        Cmd::Monitor => {
            print::status(&name, profile, &buds.state());
            let mut rx = buds.watch();
            let mut link = buds.watch_connection();
            loop {
                tokio::select! {
                    changed = rx.changed() => {
                        changed?;
                        println!("――――――――――――――――――――――――――――――――");
                        print::status(&name, profile, &rx.borrow_and_update());
                    }
                    _ = link.changed() => bail!("connection lost"),
                    _ = tokio::signal::ctrl_c() => break,
                }
            }
        }
        Cmd::Anc { mode, scene, level } => {
            let mode = match mode {
                AncArg::Off => AncMode::Off,
                AncArg::On => AncMode::NoiseCancelling {
                    scene: scene.into(),
                    level: level.unwrap_or(0),
                },
                AncArg::Transparency => {
                    let default = profile.anc.map_or(4, |a| a.default_transparency_level);
                    AncMode::Transparency {
                        level: level.unwrap_or(default),
                    }
                }
                AncArg::Voice => AncMode::VoiceTransparency,
            };
            send_and_report(buds, Command::SetAnc(mode)).await?;
            println!("Noise control: {}", describe(buds.state().anc));
        }
        Cmd::Eq { action } => match action {
            EqCmd::Show => print::eq(profile, &buds.state()),
            EqCmd::List => {
                for p in profile.eq_presets {
                    println!("{:>3}  {}", p.id, p.name);
                }
            }
            EqCmd::Preset { name } => {
                let preset = profile
                    .eq_presets
                    .iter()
                    .find(|p| p.name.eq_ignore_ascii_case(&name) || name.parse() == Ok(p.id))
                    .with_context(|| format!("unknown preset {name:?}; see `qcyctl eq list`"))?;
                if let Some(w) = preset.warning {
                    eprintln!("Note: {w}");
                }
                send_and_report(buds, Command::SetParametricEq(preset.to_parametric())).await?;
                print::eq(profile, &buds.state());
            }
            EqCmd::Custom { gains } => {
                let gains: [i8; 10] = gains.try_into().expect("clap enforces 10 values");
                send_and_report(
                    buds,
                    Command::SetParametricEq(ParametricEq::from_graphic(&gains)),
                )
                .await?;
                print::eq(profile, &buds.state());
            }
        },
        Cmd::Toggle { feature, state } => {
            let toggle = feature.into();
            if let Some(w) = profile.toggle_warning(toggle) {
                eprintln!("Note: {w}");
            }
            send_and_report(buds, Command::SetToggle(toggle, state.is_on())).await?;
            println!(
                "{}: {}",
                toggle.label(),
                on_off(buds.state().toggle(toggle))
            );
        }
        Cmd::Balance { value } => {
            send_and_report(buds, Command::SetBalance(value)).await?;
            println!("Balance: {:?}", buds.state().balance);
        }
        Cmd::PromptVolume { level } => {
            send_and_report(buds, Command::SetPromptVolume(level)).await?;
            println!("Prompt volume: {:?}", buds.state().prompt_volume);
        }
        Cmd::Language { code } => {
            let code = profile
                .prompt_languages
                .iter()
                .find(|c| c.label.eq_ignore_ascii_case(&code))
                .map_or(code.to_uppercase(), |c| c.value.to_string());
            send_and_report(buds, Command::SetPromptLanguage(code)).await?;
            println!("Prompt language: {:?}", buds.state().prompt_language);
        }
        Cmd::Rename { name } => {
            send_and_report(buds, Command::SetDeviceName(name)).await?;
            println!(
                "Name: {:?} (reconnect audio to see it on other devices)",
                buds.state().device_name
            );
        }
        Cmd::WearDetection { state, music, anc } => {
            let current = buds.state().wear_detection.unwrap_or(WearDetection {
                enabled: false,
                music: WearMusicBehavior::PauseAndResume,
                anc_follows_wear: false,
                tone: None,
            });
            let new = WearDetection {
                enabled: state.is_on(),
                music: music.map_or(current.music, Into::into),
                anc_follows_wear: anc.map_or(current.anc_follows_wear, |a| a.is_on()),
                tone: current.tone,
            };
            send_and_report(buds, Command::SetWearDetection(new)).await?;
            println!("Wear detection: {:?}", buds.state().wear_detection);
        }
        Cmd::AutoOff { minutes } => {
            let value = if minutes.eq_ignore_ascii_case("never") {
                AutoPowerOff::Never
            } else {
                AutoPowerOff::After {
                    minutes: minutes.parse().context("expected minutes or `never`")?,
                }
            };
            send_and_report(buds, Command::SetAutoPowerOff(value)).await?;
            println!(
                "Auto power off: {}",
                buds.state()
                    .auto_power_off
                    .map_or("?".into(), |a| a.label())
            );
        }
        Cmd::SleepTimer { minutes } => {
            send_and_report(buds, Command::SetPowerOffTimer(minutes)).await?;
            println!("Sleep timer: {:?}", buds.state().power_off_timer);
        }
        Cmd::Keys { action } => match action.unwrap_or(KeysCmd::Show) {
            KeysCmd::Show => print::keys(&buds.state()),
            KeysCmd::Set { gesture, action } => {
                let gesture: Gesture = gesture.into();
                send_and_report(
                    buds,
                    Command::SetKeyFunctions(vec![(gesture.id(), action.into())]),
                )
                .await?;
                print::keys(&buds.state());
            }
        },
        Cmd::Reset { what, yes } => {
            if !yes {
                bail!("this cannot be undone; add --yes to confirm");
            }
            let command = match what {
                ResetArg::Defaults => Command::ResetDefaults,
                ResetArg::Pairing => Command::ClearPairing,
                ResetArg::Factory => Command::FactoryReset,
            };
            buds.send(command).await?;
            println!("Sent.");
        }
        Cmd::Raw {
            hex,
            characteristic,
        } => {
            let data = parse_hex(&hex)?;
            let characteristic = match characteristic {
                CharArg::Command => Characteristic::Command,
                CharArg::Keys => Characteristic::Keys,
                CharArg::Eq => Characteristic::Eq,
            };
            buds.send(Command::Raw(Write {
                characteristic,
                data,
            }))
            .await?;
            tokio::time::sleep(Duration::from_millis(500)).await;
            print::status(&name, profile, &buds.state());
        }
        Cmd::Disconnect => {
            buds.disconnect().await?;
            println!("Disconnected.");
        }
    }
    Ok(())
}

/// Sends a command and waits for the earbuds' confirmation to arrive.
async fn send_and_report(buds: &Earbuds, command: Command) -> Result<()> {
    buds.send(command).await?;
    tokio::time::sleep(qcy_bluez::CONFIRM_DELAY + Duration::from_millis(400)).await;
    Ok(())
}

fn describe(anc: Option<AncMode>) -> String {
    anc.map_or("unknown".into(), AncMode::describe)
}

fn on_off(v: Option<bool>) -> &'static str {
    match v {
        Some(true) => "on",
        Some(false) => "off",
        None => "unknown",
    }
}

fn parse_hex(s: &str) -> Result<Vec<u8>> {
    let clean: String = s.chars().filter(|c| c.is_ascii_hexdigit()).collect();
    if clean.is_empty() || clean.len() % 2 != 0 {
        bail!("expected an even number of hex digits");
    }
    (0..clean.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&clean[i..i + 2], 16).map_err(Into::into))
        .collect()
}

fn init_logging(verbose: u8) {
    let default = match verbose {
        0 => "warn",
        1 => "info,qcy_bluez=debug",
        _ => "debug,qcy_bluez=trace,zbus=info",
    };
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .with_writer(std::io::stderr)
        .init();
}
