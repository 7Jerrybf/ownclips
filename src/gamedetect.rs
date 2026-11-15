// Notices any game in the foreground, for games without a watcher of their
// own (Valorant: gamestate.rs, League: league.rs). The window in front is a
// game when its program
//   - has a rule in the settings saying so (or saying it isn't),
//   - is installed in a launcher's game library (Steam, Epic, Xbox, GOG,
//     Ubisoft, EA), or
//   - runs exclusive fullscreen, which only games do.
// Only the program's path is asked for (the same access Task Manager uses):
// nothing is read from or injected into the game.
use std::ffi::c_void;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::capture;
use crate::config::{self, GameRule};

const POLL_INTERVAL: Duration = Duration::from_secs(1);
/// Epic's list of installed games is read again this often.
const EPIC_REFRESH: Duration = Duration::from_secs(60);
/// Window titles used as a game's name are cut to this many characters.
const MAX_NAME_CHARS: usize = 40;

/// Folders launchers install games into; the folder below is the game.
const LIBRARIES: [&str; 7] = [
    r"\steamapps\common\",
    r"\xboxgames\",
    r"\gog galaxy\games\",
    r"\gog games\",
    r"\ubisoft game launcher\games\",
    r"\ea games\",
    r"\origin games\",
];

/// Programs in a game library that aren't the game: launchers, crash
/// reporters, installers (matched anywhere in the file name).
const NOT_GAMES: [&str; 8] =
    ["launcher", "crash", "setup", "install", "unins", "redist", "wallpaper32", "wallpaper64"];

/// A game window in front.
#[derive(Debug, Clone, PartialEq)]
pub struct Detected {
    pub hwnd: usize,
    pub name: String,
}

/// A game installed by the Epic Games Launcher.
struct EpicGame {
    /// Lowercase, ending in a backslash.
    dir: String,
    name: String,
}

pub struct GameWatcher {
    next_poll: Instant,
    epic: Vec<EpicGame>,
    epic_read: Option<Instant>,
}

impl GameWatcher {
    pub fn new() -> Self {
        Self { next_poll: Instant::now(), epic: Vec::new(), epic_read: None }
    }

    /// The game in front right now, if any; None when it isn't time to look
    /// again yet.
    pub fn poll(&mut self) -> Option<Option<Detected>> {
        let now = Instant::now();
        if now < self.next_poll {
            return None;
        }
        self.next_poll = now + POLL_INTERVAL;
        Some(self.foreground_game())
    }

    fn foreground_game(&mut self) -> Option<Detected> {
        let cfg = config::load().ok().filter(|c| c.auto_detect_games)?;
        let window = capture::foreground_window()?;
        let exe = exe_path(window.hwnd)?;
        if self.epic_read.is_none_or(|t| t.elapsed() >= EPIC_REFRESH) {
            self.epic = epic_games();
            self.epic_read = Some(Instant::now());
        }
        let name = classify(&exe.to_string_lossy(), &cfg.game_rules, &self.epic, exclusive_fullscreen)?;
        let title: String = window.title.trim().chars().take(MAX_NAME_CHARS).collect();
        let name = name.or(Some(title).filter(|t| !t.is_empty())).unwrap_or_else(|| file_stem(&exe));
        Some(Detected { hwnd: window.hwnd, name })
    }
}

impl Default for GameWatcher {
    fn default() -> Self {
        Self::new()
    }
}

/// Whether the program at `exe` is a game to record: None if not, otherwise
/// the game's name when its library gives one.
fn classify(
    exe: &str,
    rules: &[GameRule],
    epic: &[EpicGame],
    exclusive_fullscreen: impl FnOnce() -> bool,
) -> Option<Option<String>> {
    // ASCII lowercase keeps byte positions, so names can be cut from `exe`.
    let lower = exe.to_ascii_lowercase();
    let file = lower.rsplit('\\').next().unwrap_or(&lower);
    // Valorant and League have their own watchers (and their launcher isn't a game).
    if lower.contains(r"\riot games\") || file.starts_with("valorant") || file.starts_with("league") {
        return None;
    }
    let library = || library_name(exe, &lower, epic);
    if let Some(rule) = rules.iter().find(|r| r.exe.eq_ignore_ascii_case(file)) {
        return rule.record.then(|| library());
    }
    if NOT_GAMES.iter().any(|n| file.contains(n)) {
        return None;
    }
    match library() {
        Some(name) => Some(Some(name)),
        None => exclusive_fullscreen().then_some(None),
    }
}

/// The game's name if `exe` is inside a launcher's game library.
fn library_name(exe: &str, lower: &str, epic: &[EpicGame]) -> Option<String> {
    if let Some(game) = epic.iter().find(|g| lower.starts_with(&g.dir)) {
        return Some(game.name.clone());
    }
    LIBRARIES.iter().find_map(|lib| {
        let start = lower.find(lib)? + lib.len();
        // A program directly in the library folder belongs to the launcher.
        let len = exe[start..].find('\\')?;
        Some(exe[start..start + len].to_string())
    })
}

/// Games installed by the Epic Games Launcher, from its manifests
/// (%PROGRAMDATA%\Epic\EpicGamesLauncher\Data\Manifests\*.item, JSON with
/// "InstallLocation", "DisplayName" and "AppCategories").
fn epic_games() -> Vec<EpicGame> {
    let Ok(data) = std::env::var("PROGRAMDATA") else { return Vec::new() };
    let dir = PathBuf::from(data).join(r"Epic\EpicGamesLauncher\Data\Manifests");
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("item")))
        .filter_map(|p| epic_game(&std::fs::read_to_string(p).ok()?))
        .collect()
}

fn epic_game(manifest: &str) -> Option<EpicGame> {
    let json: serde_json::Value = serde_json::from_str(manifest).ok()?;
    // The launcher also installs Unreal Engine and its plugins.
    if let Some(categories) = json["AppCategories"].as_array() {
        if !categories.iter().any(|c| c == "games") {
            return None;
        }
    }
    let location = json["InstallLocation"].as_str().filter(|l| !l.is_empty())?;
    let mut dir = location.replace('/', "\\").to_ascii_lowercase();
    if !dir.ends_with('\\') {
        dir.push('\\');
    }
    let name = json["DisplayName"].as_str().filter(|n| !n.is_empty())?.to_string();
    Some(EpicGame { dir, name })
}

fn file_stem(path: &Path) -> String {
    path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
}

// ---------- Windows ----------

#[link(name = "user32")]
extern "system" {
    fn GetWindowThreadProcessId(hwnd: *mut c_void, process_id: *mut u32) -> u32;
    fn IsWindow(hwnd: *mut c_void) -> i32;
}

#[link(name = "kernel32")]
extern "system" {
    fn OpenProcess(access: u32, inherit: i32, pid: u32) -> isize;
    fn QueryFullProcessImageNameW(process: isize, flags: u32, name: *mut u16, size: *mut u32) -> i32;
    fn CloseHandle(handle: isize) -> i32;
}

#[link(name = "shell32")]
extern "system" {
    fn SHQueryUserNotificationState(state: *mut i32) -> i32;
}

const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
const QUNS_RUNNING_D3D_FULL_SCREEN: i32 = 3;

/// Full path of the program that owns the window `hwnd`.
pub fn exe_path(hwnd: usize) -> Option<PathBuf> {
    let mut pid = 0u32;
    unsafe { GetWindowThreadProcessId(hwnd as *mut c_void, &mut pid) };
    if pid == 0 {
        return None;
    }
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if process == 0 {
        return None;
    }
    let mut buf = vec![0u16; 1024];
    let mut len = buf.len() as u32;
    let ok = unsafe { QueryFullProcessImageNameW(process, 0, buf.as_mut_ptr(), &mut len) } != 0;
    unsafe { CloseHandle(process) };
    ok.then(|| PathBuf::from(String::from_utf16_lossy(&buf[..len as usize])))
}

/// File name ("game.exe") of the program that owns the window `hwnd`.
pub fn exe_name(hwnd: usize) -> Option<String> {
    Some(exe_path(hwnd)?.file_name()?.to_string_lossy().into_owned())
}

/// Whether `hwnd` is still a window.
pub fn window_exists(hwnd: usize) -> bool {
    unsafe { IsWindow(hwnd as *mut c_void) != 0 }
}

/// Whether a program is running in exclusive fullscreen (what Windows
/// itself goes by to hold back notifications during a game).
fn exclusive_fullscreen() -> bool {
    let mut state = 0i32;
    unsafe { SHQueryUserNotificationState(&mut state) == 0 && state == QUNS_RUNNING_D3D_FULL_SCREEN }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(exe: &str, rules: &[GameRule], fullscreen: bool) -> Option<Option<String>> {
        let epic = [EpicGame { dir: r"d:\epic\fortnite\".into(), name: "Fortnite".into() }];
        classify(exe, rules, &epic, || fullscreen)
    }

    fn rule(exe: &str, record: bool) -> GameRule {
        GameRule { exe: exe.into(), record }
    }

    #[test]
    fn library_games_are_named_after_their_folder() {
        let exe = r"D:\SteamLibrary\steamapps\common\ELDEN RING\Game\eldenring.exe";
        assert_eq!(check(exe, &[], false), Some(Some("ELDEN RING".into())));
        let exe = r"D:\Epic\Fortnite\FortniteGame\Binaries\Win64\FortniteClient-Win64-Shipping.exe";
        assert_eq!(check(exe, &[], false), Some(Some("Fortnite".into())));
    }

    #[test]
    fn other_programs_need_fullscreen_or_a_rule() {
        let exe = r"C:\Games\Foo\foo.exe";
        assert_eq!(check(exe, &[], false), None);
        assert_eq!(check(exe, &[], true), Some(None));
        assert_eq!(check(exe, &[rule("FOO.exe", true)], false), Some(None));
    }

    #[test]
    fn rules_and_built_in_exceptions_turn_games_off() {
        let exe = r"C:\Program Files (x86)\Steam\steamapps\common\Stellaris\stellaris.exe";
        assert_eq!(check(exe, &[rule("stellaris.exe", false)], true), None);
        assert_eq!(check(r"C:\Program Files (x86)\Steam\steam.exe", &[], false), None);
        assert_eq!(check(r"C:\Steam\steamapps\common\Stellaris\dowser-launcher.exe", &[], false), None);
        assert_eq!(check(r"C:\Riot Games\VALORANT\live\VALORANT.exe", &[], true), None);
        assert_eq!(check(r"D:\Games\League of Legends.exe", &[], true), None);
    }

    #[test]
    fn epic_manifests() {
        let game = epic_game(r#"{"InstallLocation": "D:/Epic/Fortnite", "DisplayName": "Fortnite", "AppCategories": ["public", "games"]}"#).unwrap();
        assert_eq!((game.dir.as_str(), game.name.as_str()), (r"d:\epic\fortnite\", "Fortnite"));
        assert!(epic_game(r#"{"InstallLocation": "D:\\UE_5.4", "DisplayName": "Unreal Engine", "AppCategories": ["engines"]}"#).is_none());
    }
}
