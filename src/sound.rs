//! Which sound each agent change makes, and how thick the rain is. Free of host calls.

use crate::fleet::{Change, Fleet, State};

#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    pub chimes: bool,
    pub bells: bool,
    pub rain: bool,
    /// 0 to 1, for every sound.
    pub volume: f32,
    /// This many running agents make the heaviest rain.
    pub rain_full_at: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Self { chimes: true, bells: true, rain: false, volume: 0.5, rain_full_at: 6 }
    }
}

/// C major pentatonic over two octaves, in semitones from Haunt's chime (a C); Haunt plays ±12.
const SCALE: [(i32, &str); 10] = [(-12, "C"), (-10, "D"), (-8, "E"), (-5, "G"), (-3, "A"), (0, "C"), (2, "D"), (4, "E"), (7, "G"), (9, "A")];

#[derive(Debug, Clone, PartialEq)]
pub enum Cue {
    /// An agent needs you: its own note.
    Chime { semitones: i32, note: &'static str },
    /// An agent finished.
    Bell,
    /// An agent failed: the bell, a fourth lower.
    LowBell,
}

/// The same name always gets the same note (FNV-1a over its bytes).
pub fn note_for(name: &str) -> (i32, &'static str) {
    let mut h: u32 = 0x811c_9dc5;
    for b in name.bytes() {
        h = (h ^ u32::from(b)).wrapping_mul(0x0100_0193);
    }
    SCALE[(h % SCALE.len() as u32) as usize]
}

pub fn cue(change: Change, name: &str, s: &Settings) -> Option<Cue> {
    if change.from.is_none() && change.to != State::NeedsYou {
        return None;
    }
    match change.to {
        State::NeedsYou if s.chimes => {
            let (semitones, note) = note_for(name);
            Some(Cue::Chime { semitones, note })
        }
        State::Done if s.bells => Some(Cue::Bell),
        State::Failed if s.bells => Some(Cue::LowBell),
        _ => None,
    }
}

/// 0 (silent) to 1, rising with each running agent.
pub fn rain_level(fleet: &Fleet, s: &Settings) -> f32 {
    if !s.rain {
        return 0.0;
    }
    let running = fleet.count(State::Running) as f32;
    (running / s.rain_full_at.max(1) as f32).min(1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fleet::Ev;

    #[test]
    fn notes_are_stable_and_in_the_scale() {
        assert_eq!(note_for("api"), note_for("api"));
        let names = ["api", "web", "docs", "infra", "ml", "shop"];
        assert!(names.iter().all(|n| SCALE.contains(&note_for(n))));
        assert!(names.iter().map(|n| note_for(n)).collect::<std::collections::HashSet<_>>().len() > 2, "names spread over the scale");
    }

    #[test]
    fn changes_make_their_sounds() {
        let s = Settings::default();
        let c = |from, to| Change { from, to };
        assert!(matches!(cue(c(Some(State::Running), State::NeedsYou), "api", &s), Some(Cue::Chime { .. })));
        assert_eq!(cue(c(Some(State::Running), State::Done), "api", &s), Some(Cue::Bell));
        assert_eq!(cue(c(Some(State::Running), State::Failed), "api", &s), Some(Cue::LowBell));
        assert_eq!(cue(c(Some(State::Done), State::Running), "api", &s), None, "starting work is the rain's job");
        assert_eq!(cue(c(None, State::Done), "api", &s), None, "a first event never rings");
        let quiet = Settings { chimes: false, bells: false, ..s };
        assert_eq!(cue(c(Some(State::Running), State::NeedsYou), "api", &quiet), None);
        assert_eq!(cue(c(Some(State::Running), State::Done), "api", &quiet), None);
    }

    #[test]
    fn rain_is_off_by_default_and_thickens_with_running_agents() {
        assert!(!Settings::default().rain);
        assert_eq!(rain_level(&Fleet::default(), &Settings::default()), 0.0);
        let s = Settings { rain: true, ..Settings::default() };
        let mut f = Fleet::default();
        assert_eq!(rain_level(&f, &s), 0.0);
        for (i, n) in ["a", "b", "c"].iter().enumerate() {
            f.apply(&Ev { surface: n, kind: "prompt_submit", ..Ev::default() }, i as u64);
        }
        assert_eq!(rain_level(&f, &s), 0.5);
        assert_eq!(rain_level(&f, &Settings { rain_full_at: 2, ..s.clone() }), 1.0);
        assert_eq!(rain_level(&f, &Settings { rain: false, ..s }), 0.0);
    }
}
