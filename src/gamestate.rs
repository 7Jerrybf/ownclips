// Follows Valorant's own log (%LOCALAPPDATA%\VALORANT\Saved\Logs\
// ShooterGame.log) to know when a match starts and ends, and on which map
// and mode. Reading the log file doesn't touch the game process at all.
//
// Lines this relies on (seen in real logs):
//   LogMapLoadModel: Update: [Map Name: Infinity | ...] [URL: ...?game=/Game/GameModes/Deathmatch/...]
//   LogGameFlowStateManager: ... Broadcasting state changed to InGame
//   LogShooterGameState: Match Ended: Completion State: 'Completed' ...
//   LogGameFlowStateManager: ... Broadcasting state changed to TransitionToMainMenu
// The log is recreated each time the game starts. When the previous log
// can't be rotated away the game writes ShooterGame_2.log (etc.) instead,
// so the watcher follows whichever ShooterGame*.log has the newest entry.
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::session::EventKind;

/// Which game a match is from.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Game {
    #[default]
    Valorant,
    League,
}

#[derive(Debug, Clone, PartialEq)]
pub enum GameEvent {
    MatchStarted { map: String, mode: String },
    /// One of your kills, deaths or assists, `ago` seconds before now. Only
    /// games that report these themselves (League); Valorant's come from
    /// the killfeed on screen.
    Event { kind: EventKind, ago: f64 },
    /// The match is decided; the game stays in the match a few more seconds.
    MatchEnded,
    /// Back to the menus (after a match, or by leaving one).
    LeftMatch,
}

pub struct LogWatcher {
    dir: PathBuf,
    /// The log currently being followed.
    path: Option<PathBuf>,
    pos: u64,
    partial: String,
    map: String,
    mode: String,
    in_game: bool,
    /// Already in a match when the watcher started; reported on first poll.
    pending_start: bool,
}

impl LogWatcher {
    pub fn new() -> Result<Self> {
        let local = std::env::var("LOCALAPPDATA").context("找不到 LOCALAPPDATA 環境變數")?;
        let dir = PathBuf::from(local).join(r"VALORANT\Saved\Logs");
        let mut w = Self {
            dir,
            path: None,
            pos: 0,
            partial: String::new(),
            map: String::new(),
            mode: String::new(),
            in_game: false,
            pending_start: false,
        };
        // Catch up on the existing log silently, so starting the app in the
        // middle of a match still records the rest of it. A log that went
        // quiet while "in game" is from a game that crashed or was closed.
        let _ = w.read_new_lines();
        let live = w.path.as_deref().is_some_and(|p| written_recently(p, chrono::Duration::minutes(5)));
        w.in_game &= live;
        w.pending_start = w.in_game;
        Ok(w)
    }

    /// Reads whatever the game appended since the last poll.
    pub fn poll(&mut self) -> Vec<GameEvent> {
        let mut events = Vec::new();
        if std::mem::take(&mut self.pending_start) {
            events.push(self.started());
        }
        events.extend(self.read_new_lines().unwrap_or_default());
        events
    }

    fn started(&self) -> GameEvent {
        GameEvent::MatchStarted { map: map_display_name(&self.map), mode: mode_display_name(&self.mode) }
    }

    fn read_new_lines(&mut self) -> Result<Vec<GameEvent>> {
        let mut events = Vec::new();
        let current = newest_log(&self.dir).context("找不到 Valorant 記錄檔")?;
        if self.path.as_ref() != Some(&current) {
            // The game restarted into a different log file.
            if self.path.is_some() && self.in_game {
                events.push(GameEvent::LeftMatch);
            }
            self.path = Some(current.clone());
            self.pos = 0;
            self.partial.clear();
            self.in_game = false;
        }
        let mut file = File::open(&current)?;
        let len = file.metadata()?.len();
        if len < self.pos {
            // The game restarted and began a new log.
            if self.in_game {
                events.push(GameEvent::LeftMatch);
            }
            self.pos = 0;
            self.partial.clear();
            self.in_game = false;
        }
        file.seek(SeekFrom::Start(self.pos))?;
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)?;
        self.pos += bytes.len() as u64;
        self.partial.push_str(&String::from_utf8_lossy(&bytes));

        let Some(last_newline) = self.partial.rfind('\n') else {
            return Ok(events);
        };
        let complete: String = self.partial.drain(..=last_newline).collect();
        for line in complete.lines() {
            if let Some(e) = self.parse_line(line) {
                events.push(e);
            }
        }
        Ok(events)
    }

    fn parse_line(&mut self, line: &str) -> Option<GameEvent> {
        if line.contains("LogMapLoadModel: Update:") {
            if let Some(map) = between(line, "[Map Name: ", " |") {
                self.map = map.to_string();
            }
            if let Some(mode) = between(line, "game=/Game/GameModes/", "/") {
                self.mode = mode.to_string();
            }
        } else if line.contains("Broadcasting state changed to InGame") {
            if !self.in_game {
                self.in_game = true;
                return Some(self.started());
            }
        } else if line.contains("LogShooterGameState: Match Ended") {
            if self.in_game {
                return Some(GameEvent::MatchEnded);
            }
        } else if line.contains("Broadcasting state changed to TransitionToMainMenu")
            || line.contains("Loopstate changed from INGAME to MENUS")
        {
            if self.in_game {
                self.in_game = false;
                return Some(GameEvent::LeftMatch);
            }
        }
        None
    }
}

/// The ShooterGame*.log (not a backup) whose last line is the most recent.
/// File modification times can't be used: they lag while the game keeps
/// the file open.
fn newest_log(dir: &Path) -> Option<PathBuf> {
    std::fs::read_dir(dir)
        .ok()?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            name.starts_with("ShooterGame") && name.ends_with(".log") && !name.contains("backup")
        })
        .filter_map(|p| Some((last_timestamp(&p)?, p)))
        .max_by(|a, b| a.0.cmp(&b.0))
        .map(|(_, p)| p)
}

/// The "[2026.10.21-13.46.33:170]" stamp starting the file's last
/// timestamped line; these sort correctly as strings. (Only line starts
/// count: text like "Response Code: [200]" appears inside lines.)
fn last_timestamp(path: &Path) -> Option<String> {
    let mut file = File::open(path).ok()?;
    let len = file.metadata().ok()?.len();
    file.seek(SeekFrom::Start(len.saturating_sub(64 * 1024))).ok()?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).ok()?;
    let text = String::from_utf8_lossy(&bytes);
    text.lines().rev().find_map(|line| {
        let stamp = line.get(..25)?;
        let b = stamp.as_bytes();
        (b[0] == b'[' && b[24] == b']' && b[11] == b'-' && b[1..5].iter().all(u8::is_ascii_digit))
            .then(|| stamp.to_string())
    })
}

/// Whether a log's last line was written within `max_age` (log times are UTC).
fn written_recently(path: &Path, max_age: chrono::Duration) -> bool {
    last_timestamp(path)
        .and_then(|s| chrono::NaiveDateTime::parse_from_str(&s[1..20], "%Y.%m.%d-%H.%M.%S").ok())
        .is_some_and(|t| chrono::Utc::now().naive_utc() - t < max_age)
}

fn between<'a>(s: &'a str, start: &str, end: &str) -> Option<&'a str> {
    let rest = &s[s.find(start)? + start.len()..];
    Some(&rest[..rest.find(end)?])
}

/// The log uses internal codenames for maps.
fn map_display_name(code: &str) -> String {
    let name = match code {
        "Ascent" => "Ascent",
        "Duality" => "Bind",
        "Triad" => "Haven",
        "Bonsai" => "Split",
        "Port" => "Icebox",
        "Foxtrot" => "Breeze",
        "Canyon" => "Fracture",
        "Pitt" => "Pearl",
        "Jam" => "Lotus",
        "Juliett" => "Sunset",
        "Infinity" => "Abyss",
        "Rook" => "Corrode",
        "Range" | "RangeV2" => "靶場",
        "NPEV2" => "新手教學",
        "HURM_Alley" => "District",
        "HURM_Yard" => "Piazza",
        "HURM_Bowl" => "Kasbah",
        "HURM_Helix" => "Drift",
        "HURM_HighTide" => "Glitch",
        "" => "未知地圖",
        other => other,
    };
    name.to_string()
}

fn mode_display_name(code: &str) -> String {
    let name = match code {
        "Bomb" => "一般 / 競技",
        "Deathmatch" => "死鬥",
        "QuickBomb" => "輕速衝點",
        "GunGame" => "攻防升級",
        "HURM" => "團隊死鬥",
        "OneForAll" => "複製大亂鬥",
        "SnowballFight" => "雪球大戰",
        "ShootingRange" => "靶場",
        "NewPlayerExperience" | "NPEV2" => "新手教學",
        "" => "未知模式",
        other => other,
    };
    name.to_string()
}

/// Modes not worth recording (practice, or watching someone else).
pub fn is_practice(mode: &str) -> bool {
    matches!(mode, "靶場" | "新手教學" | "練習工具" | "觀戰 / 重播")
}
