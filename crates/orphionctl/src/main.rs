//! `orphionctl` – command line control for QCY earbuds.
//!
//! Every invocation finds the earbuds, opens the BLE control link, performs
//! one action and (unless `--keep-connected`) closes the link again.

mod cli;
mod controls;
mod print;

use std::time::Duration;

use anyhow::{Context, Result, bail};
use clap::Parser;
use qcy_client::transport::Backend;
use qcy_client::{Earbuds, Platform, Selector};
use qcy_protocol::panel::{ActionKind, ItemKind};
use qcy_protocol::types::{WearDetection, WearMusicBehavior};
use qcy_protocol::{Characteristic, Command, PanelWrite, Write, catalog};
use tracing_subscriber::EnvFilter;

use crate::cli::{CharArg, Cli, Cmd, EqCmd, KeysCmd, ResetArg};

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    init_logging(cli.verbose);

    // Catalogue lookups need no Bluetooth.
    match &cli.command {
        Some(Cmd::Models { filter }) => {
            let filter = filter.as_deref().map(str::to_lowercase);
            for m in catalog::models() {
                if filter
                    .as_ref()
                    .is_none_or(|f| m.name.to_lowercase().contains(f))
                {
                    println!("{:>6}  {:<8} {}", m.id, format!("{:?}", m.chip), m.name);
                }
            }
            return Ok(());
        }
        Some(Cmd::Panel { model: Some(id) }) => {
            print::panel(*id, &catalog::panel(*id));
            return Ok(());
        }
        _ => {}
    }

    let platform = Platform::init()
        .await
        .context("cannot reach the Bluetooth stack")?;
    let timeout = Duration::from_secs(cli.timeout);

    if matches!(cli.command, Some(Cmd::Scan)) {
        println!("Scanning for {}s…", cli.timeout.min(6));
        let found = platform.scan(timeout.min(Duration::from_secs(6))).await?;
        if found.is_empty() {
            println!("No QCY earbuds found. Are they out of the case?");
        }
        found.iter().for_each(print::endpoint);
        return Ok(());
    }

    let selector = cli.device.map_or(Selector::Auto, Selector::Address);
    let endpoint = platform.find(&selector, timeout).await?;
    let buds = Earbuds::connect(&platform, &endpoint).await?;
    // Give replies to the initial requests a moment to arrive.
    tokio::time::sleep(Duration::from_millis(600)).await;

    let result = run(&buds, cli.command.unwrap_or(Cmd::Status)).await;

    if !cli.keep_connected {
        buds.disconnect().await.ok();
    }
    result
}

async fn run(buds: &Earbuds, cmd: Cmd) -> Result<()> {
    let panel = buds.panel();
    let model = buds.endpoint().advertisement.vendor_id;
    let name = buds.endpoint().display_name();
    let has = |c: Characteristic| buds.has(c);
    let status = |s: &qcy_protocol::state::DeviceState| print::status(&name, model, &panel, s, has);

    match cmd {
        Cmd::Scan | Cmd::Models { .. } => unreachable!("handled before connecting"),
        Cmd::Panel { .. } => print::panel(model, &panel),
        Cmd::Status => status(&buds.state()),
        Cmd::Monitor => {
            status(&buds.state());
            let mut rx = buds.watch();
            let mut link = buds.watch_connection();
            loop {
                tokio::select! {
                    changed = rx.changed() => {
                        changed?;
                        println!("――――――――――――――――――――――――――――――――");
                        status(&rx.borrow_and_update());
                    }
                    _ = link.changed() => bail!("connection lost"),
                    _ = tokio::signal::ctrl_c() => break,
                }
            }
        }
        Cmd::Anc { mode, scene, level } => {
            let Some(mode) = mode else {
                controls::list_anc(&panel);
                return Ok(());
            };
            let command = controls::anc(&panel, &mode, scene.as_deref(), level)?;
            send_and_report(buds, command).await?;
            println!(
                "Noise control: {}",
                controls::describe_anc(&panel, &buds.state()).unwrap_or_else(|| "unknown".into())
            );
        }
        Cmd::Eq { action } => {
            let eq = panel.eq.as_ref().context("this model has no equalizer")?;
            match action.unwrap_or(EqCmd::Show) {
                EqCmd::Show => print::eq(eq, &buds.state()),
                EqCmd::List => {
                    for p in &eq.presets {
                        println!("{:>3}  {}", p.id, p.name);
                    }
                }
                EqCmd::Preset { name, yes } => {
                    let preset = eq
                        .presets
                        .iter()
                        .find(|p| {
                            controls::slug(&p.name) == controls::slug(&name)
                                || name.parse() == Ok(p.id)
                        })
                        .with_context(|| {
                            format!("unknown preset {name:?}; see `orphionctl eq list`")
                        })?;
                    if let Some(w) = &preset.warning
                        && !yes
                    {
                        bail!("{w}\nAdd --yes to apply it anyway.");
                    }
                    for command in eq.apply_preset(preset) {
                        send_and_report(buds, command).await?;
                    }
                    print::eq(eq, &buds.state());
                }
                EqCmd::Game { name } => {
                    let game = panel
                        .game_eq
                        .as_ref()
                        .context("this model has no game EQ")?;
                    let preset = game
                        .presets
                        .iter()
                        .find(|p| controls::slug(&p.name).starts_with(&controls::slug(&name)))
                        .with_context(|| format!("unknown game preset {name:?}"))?;
                    for command in game.apply_preset(preset) {
                        send_and_report(buds, command).await?;
                    }
                }
                EqCmd::Custom { gains } => {
                    if gains.len() != eq.bands.len() {
                        let labels: Vec<&str> = eq.bands.iter().map(|b| b.label.as_str()).collect();
                        bail!(
                            "this model's EQ has {} bands ({}); got {} gains",
                            eq.bands.len(),
                            labels.join(", "),
                            gains.len()
                        );
                    }
                    send_and_report(buds, eq.apply_custom(&gains)).await?;
                    print::eq(eq, &buds.state());
                }
            }
        }
        Cmd::Set { name, value, yes } => {
            let settings = controls::settings(&panel);
            let (Some(name), Some(value)) = (name, value) else {
                let s = buds.state();
                for (slug, item) in &settings {
                    println!(
                        "{slug:<28} {:<24} [{}]",
                        controls::value(item, &s),
                        controls::accepted(item)
                    );
                }
                return Ok(());
            };
            let item = *controls::find(&settings, &name, "setting")?;
            let command = controls::command(item, &value, &buds.state())?;
            if controls::restarts(item) && !yes {
                bail!(
                    "changing {} restarts the earbuds; add --yes to go ahead",
                    item.title
                );
            }
            if let ItemKind::Switch(sw) = &item.kind
                && let Some(w) = &sw.warning
            {
                eprintln!("Note: {w}");
            }
            send_and_report(buds, command).await?;
            println!("{}: {}", item.title, controls::value(item, &buds.state()));
        }
        Cmd::Balance { value } => {
            send_and_report(buds, Command::SetBalance(value)).await?;
            println!("Balance: {:?}", buds.state().balance);
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
        Cmd::Keys { action } => match action.unwrap_or(KeysCmd::Show) {
            KeysCmd::Show => print::keys(&panel, &buds.state(), has),
            KeysCmd::Set { gesture, action } => {
                let gestures = controls::gestures(&panel, has);
                let slot = *controls::find(&gestures, &gesture, "gesture")?;
                send_and_report(buds, controls::key_command(slot, &action)?).await?;
                print::keys(&panel, &buds.state(), has);
            }
        },
        Cmd::Reset { what, yes } => {
            if !yes {
                bail!("this cannot be undone; add --yes to confirm");
            }
            let wanted = match what {
                ResetArg::Defaults => ActionKind::ResetDefaults,
                ResetArg::Pairing => ActionKind::ClearPairing,
                ResetArg::Factory => ActionKind::FactoryReset,
            };
            // The model's own write for it, which may use an older
            // characteristic than the generic command.
            let write = panel
                .items
                .iter()
                .find_map(|i| match &i.kind {
                    ItemKind::Action(a) if a.kind == wanted => Some(a.write.clone()),
                    _ => None,
                })
                .context("this model does not offer that")?;
            buds.send(Command::Panel(PanelWrite {
                write,
                setting: None,
                value: None,
            }))
            .await?;
            println!("Sent.");
        }
        Cmd::Raw {
            hex,
            characteristic,
        } => {
            let data = parse_hex(&hex)?;
            let characteristic = match characteristic {
                CharArg::Command => Characteristic::COMMAND,
                CharArg::Keys => Characteristic::KEYS,
                CharArg::Eq => Characteristic::EQ,
            };
            buds.send(Command::Raw(Write {
                characteristic,
                data,
            }))
            .await?;
            tokio::time::sleep(Duration::from_millis(500)).await;
            status(&buds.state());
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
    tokio::time::sleep(qcy_client::CONFIRM_DELAY + Duration::from_millis(400)).await;
    Ok(())
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
        1 => "info,qcy_client=debug",
        _ => "debug,qcy_client=trace,zbus=info",
    };
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .with_writer(std::io::stderr)
        .init();
}
