// OwnClips core: built-in recording, match detection from the game log
// (Valorant) or the game's local API (League), killfeed event detection and clip cutting. Used by the desktop app
// (app/src-tauri) and the command-line tool (src/main.rs).
pub mod audio;
pub mod capture;
pub mod clip;
pub mod config;
pub mod gamestate;
pub mod killfeed;
pub mod league;
pub mod priority;
pub mod qpc;
pub mod recorder;
pub mod session;
pub mod storage;
