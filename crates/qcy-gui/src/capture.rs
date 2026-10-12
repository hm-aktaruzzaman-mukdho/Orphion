//! `--screenshots DIR`: once connected, render a fixed list of screens (every
//! tab plus a few interactive states), save each as `DIR/<name>.ppm` and quit.
//! Used for documentation screenshots and to review the UI without clicking
//! through it (pair with `--demo` to run without earbuds).

use std::path::PathBuf;
use std::time::{Duration, Instant};

use eframe::egui::{self, ColorImage, UserData, ViewportCommand};

use crate::views::Tab;

const SETTLE: Duration = Duration::from_millis(1500);

/// One screen to capture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shot {
    Tab(Tab),
    /// The Sound tab with the custom EQ editor open.
    CustomEq,
    /// A confirmation dialog on top of the Settings tab.
    Dialog,
}

impl Shot {
    fn file_name(self) -> String {
        match self {
            Shot::Tab(t) => format!("{t:?}").to_lowercase(),
            Shot::CustomEq => "sound-custom".into(),
            Shot::Dialog => "dialog".into(),
        }
    }
}

pub struct Capture {
    dir: PathBuf,
    remaining: Vec<Shot>,
    current: Option<Shot>,
    since: Option<Instant>,
    requested: bool,
}

impl Capture {
    pub fn from_args() -> Option<Self> {
        let dir = crate::arg_value("--screenshots")?;
        let mut shots: Vec<Shot> = Tab::ALL.into_iter().map(Shot::Tab).collect();
        shots.extend([Shot::CustomEq, Shot::Dialog]);
        shots.reverse();
        Some(Self {
            dir: dir.into(),
            remaining: shots,
            current: None,
            since: None,
            requested: false,
        })
    }

    /// Drives the capture; returns the screen the app should show.
    pub fn step(&mut self, ctx: &egui::Context, connected: bool) -> Option<Shot> {
        ctx.request_repaint_after(Duration::from_millis(100));
        if !connected {
            self.since = None;
            return None;
        }

        // Save a screenshot delivered by eframe.
        let image = ctx.input(|i| {
            i.raw.events.iter().find_map(|e| match e {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        if let (Some(image), Some(done)) = (image, self.current) {
            let path = self.dir.join(format!("{}.ppm", done.file_name()));
            if let Err(e) = save_ppm(&path, &image) {
                tracing::error!("saving screenshot failed: {e}");
            }
            self.current = None;
            self.requested = false;
        }

        if self.current.is_none() {
            match self.remaining.pop() {
                Some(next) => {
                    self.current = Some(next);
                    self.since = Some(Instant::now());
                }
                None => {
                    ctx.send_viewport_cmd(ViewportCommand::Close);
                    return None;
                }
            }
        }
        let since = *self.since.get_or_insert_with(Instant::now);
        if !self.requested && since.elapsed() > SETTLE {
            ctx.send_viewport_cmd(ViewportCommand::Screenshot(UserData::default()));
            self.requested = true;
        }
        self.current
    }
}

fn save_ppm(path: &std::path::Path, image: &ColorImage) -> std::io::Result<()> {
    std::fs::create_dir_all(path.parent().unwrap_or(std::path::Path::new(".")))?;
    let [w, h] = image.size;
    let mut data = format!("P6\n{w} {h}\n255\n").into_bytes();
    for px in &image.pixels {
        data.extend_from_slice(&[px.r(), px.g(), px.b()]);
    }
    std::fs::write(path, data)
}
