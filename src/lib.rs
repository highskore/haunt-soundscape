//! Soundscape: each agent has its own soft chime when it needs you, a bell rings when one finishes,
//! and an optional rain thickens as more agents run.
// wit-bindgen's flattened `notify` import has 14 params.
#![allow(clippy::too_many_arguments)]

wit_bindgen::generate!({ path: "wit/haunt-plugin.wit", world: "plugin" });

// Shared with the other fleet-following plugins; not every helper is used here.
#[allow(dead_code)]
mod fleet;
mod sound;

use std::sync::Mutex;

use fleet::{Ev, Fleet};
use haunt::plugin::log::log;
use haunt::plugin::timer;
use haunt::plugin::sound::{self as audio, Name};
use haunt::plugin::types::LogLevel;
use serde::Deserialize;
use sound::Cue;

/// The bell a fourth lower for a failed turn.
const LOW_BELL: i32 = -5;

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default)]
pub struct Config {
    pub chimes: bool,
    pub bells: bool,
    pub rain: bool,
    /// Percent, for every sound.
    pub volume: i64,
    pub rain_full_at: i64,
    pub unverified: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self { chimes: true, bells: true, rain: false, volume: 50, rain_full_at: 6, unverified: false }
    }
}

impl Config {
    pub fn sound(&self) -> sound::Settings {
        sound::Settings { chimes: self.chimes, bells: self.bells, rain: self.rain, volume: self.volume.clamp(0, 100) as f32 / 100.0, rain_full_at: self.rain_full_at.clamp(1, 50) as u32 }
    }
}

#[derive(Default)]
struct Live {
    config: Config,
    fleet: Fleet,
    rain: f32,
}

static LIVE: Mutex<Option<Live>> = Mutex::new(None);

fn with<R>(f: impl FnOnce(&mut Live) -> R) -> R {
    f(LIVE.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(Live::default))
}

fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}

/// What `sound.play` gets for a cue.
pub fn play_args(c: &Cue) -> (Name, i32) {
    match c {
        Cue::Chime { semitones, .. } => (Name::Chime, *semitones),
        Cue::Bell => (Name::Bell, 0),
        Cue::LowBell => (Name::Bell, LOW_BELL),
    }
}

/// A refused change (at most 4 a second) is retried on the timer, so rain never outlasts the agents.
fn rain(l: &mut Live) {
    let s = l.config.sound();
    let level = sound::rain_level(&l.fleet, &s) * s.volume;
    if (level - l.rain).abs() <= f32::EPSILON {
        return;
    }
    match audio::ambient(Name::Rain, level) {
        Ok(()) => {
            l.rain = level;
            timer::stop();
        }
        Err(e) => {
            log(LogLevel::Warn, &format!("soundscape: no rain: {e}"));
            let _ = timer::every(15);
        }
    }
}

struct Plugin;

impl Guest for Plugin {
    fn init(json: String) {
        with(|l| l.config = serde_json::from_str(&json).unwrap_or_default());
    }

    fn on_settings_changed(json: String) {
        with(|l| {
            l.config = serde_json::from_str(&json).unwrap_or_default();
            rain(l);
        });
    }

    fn on_event(event: AgentEvent) {
        let Some(surface) = event.surface.as_deref() else { return };
        with(|l| {
            if !event.verified && !l.config.unverified {
                return;
            }
            let ev = Ev { surface, kind: &event.kind, cwd: event.cwd.as_deref(), summary: event.summary.as_deref(), subagent: event.agent_id.is_some() };
            let Some(change) = l.fleet.apply(&ev, now_ms()) else { return };
            let s = l.config.sound();
            let name = l.fleet.agents.get(surface).map(|a| a.name.clone()).unwrap_or_default();
            if let Some(c) = sound::cue(change, &name, &s) {
                let (n, pitch) = play_args(&c);
                if let Err(e) = audio::play(n, pitch, s.volume) {
                    log(LogLevel::Warn, &format!("soundscape: not played: {e}"));
                }
            }
            rain(l);
        });
    }

    fn palette_items(_mode: String, _query: String) -> Vec<PaletteItem> {
        Vec::new()
    }

    fn on_action(_action_id: String, _payload: String) {}

    fn row_menu(_surface: String) -> Vec<MenuItem> {
        Vec::new()
    }

    fn on_row_moved(_surface: String, _group: String, _before: Option<String>) {}

    fn on_theme_changed() {}

    fn on_tick(_now_ms: u64) {
        with(rain);
    }

    fn on_exec(_job: u64, _outcome: Result<haunt::plugin::exec::Output, String>) {}

    fn on_away_changed(_away: bool, _away_ms: u64) {}

    fn on_hover_hold(_held: bool, _surface: String) {}

    fn on_fs_tree(_job: u64, _outcome: Result<haunt::plugin::fs_list::TreeResult, String>) {}
}

export!(Plugin);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cues_map_to_haunts_sounds() {
        assert!(matches!(play_args(&Cue::Chime { semitones: 7, note: "G" }), (Name::Chime, 7)));
        assert!(matches!(play_args(&Cue::Bell), (Name::Bell, 0)));
        assert!(matches!(play_args(&Cue::LowBell), (Name::Bell, -5)));
        for n in ["api", "web", "docs", "infra"] {
            assert!((-12..=12).contains(&sound::note_for(n).0), "Haunt plays within an octave either way");
        }
    }

    #[test]
    fn manifest_asks_for_what_it_uses_and_defaults_match() {
        let m = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/haunt-plugin.toml")).unwrap();
        let t: toml::Table = toml::from_str(&m).unwrap();
        let caps = t["capabilities"].as_table().unwrap();
        assert_eq!(caps.keys().collect::<Vec<_>>(), ["events", "sound", "timer"]);
        let defaults: serde_json::Map<String, serde_json::Value> =
            t["settings"].as_table().unwrap().iter().map(|(k, v)| (k.clone(), serde_json::to_value(&v["default"]).unwrap())).collect();
        let c = serde_json::from_value::<Config>(serde_json::Value::Object(defaults)).unwrap();
        assert_eq!(c, Config::default());
        assert_eq!(c.sound(), sound::Settings::default());
    }
}
