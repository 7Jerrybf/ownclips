// Recordings folder size limit: once the finished recordings add up to more
// than the limit, the oldest are deleted (for good: the recycle bin would
// keep the space taken) until they fit. Matches marked "keep", recordings
// still in progress and exported clips are never touched.
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::config;
use crate::session::MatchInfo;

pub const GB: u64 = 1024 * 1024 * 1024;

/// One finished recording and the files that belong to it.
pub struct Recording {
    pub video: PathBuf,
    files: Vec<PathBuf>,
    pub bytes: u64,
    pub keep: bool,
}

/// What a cleanup would do.
pub struct Plan {
    /// Size of all finished recordings.
    pub used: u64,
    /// Oldest first.
    pub delete: Vec<Recording>,
}

impl Plan {
    pub fn freed(&self) -> u64 {
        self.delete.iter().map(|r| r.bytes).sum()
    }
}

/// Finished recordings in `dir`, oldest first (names start with the date).
/// One that still has its .mkv is being recorded or muxed.
fn recordings(dir: &Path) -> Vec<Recording> {
    let mut out: Vec<Recording> = fs::read_dir(dir)
        .map(|rd| rd.filter_map(|e| e.ok().map(|e| e.path())).collect::<Vec<_>>())
        .unwrap_or_default()
        .into_iter()
        .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("mp4")))
        .filter(|p| !p.with_extension("mkv").exists())
        .filter_map(|video| {
            let info = MatchInfo::load(&video)?;
            let files: Vec<PathBuf> =
                [video.clone(), MatchInfo::path_for(&video), video.with_extension("events.orig.json")]
                    .into_iter()
                    .filter(|f| f.exists())
                    .collect();
            let bytes = files.iter().filter_map(|f| fs::metadata(f).ok()).map(|m| m.len()).sum();
            Some(Recording { video, files, bytes, keep: info.keep })
        })
        .collect();
    out.sort_by(|a, b| a.video.file_name().cmp(&b.video.file_name()));
    out
}

/// The oldest recordings (not kept) to delete to get within `limit` bytes.
pub fn plan(dir: &Path, limit: Option<u64>) -> Plan {
    let all = recordings(dir);
    let used: u64 = all.iter().map(|r| r.bytes).sum();
    let mut over = limit.map_or(0, |l| used.saturating_sub(l));
    let delete = all
        .into_iter()
        .filter(|r| !r.keep)
        .take_while(|r| {
            let take = over > 0;
            over = over.saturating_sub(r.bytes);
            take
        })
        .collect();
    Plan { used, delete }
}

/// Deletes per [`plan`] with the configured limit; returns how many
/// recordings were deleted and how many bytes that freed.
pub fn enforce() -> Result<(usize, u64)> {
    let cfg = config::load()?;
    let Some(limit) = cfg.storage_limit() else { return Ok((0, 0)) };
    let plan = plan(&cfg.recordings_dir()?, Some(limit));
    let (mut count, mut freed) = (0, 0);
    for r in &plan.delete {
        // The video goes first: a recording without it isn't listed anymore.
        if fs::remove_file(&r.video).is_ok() {
            for f in r.files.iter().filter(|f| **f != r.video) {
                let _ = fs::remove_file(f);
            }
            count += 1;
            freed += r.bytes;
        }
    }
    Ok((count, freed))
}

/// Marks a match as kept (never deleted by the size limit) or not.
pub fn set_keep(video: &Path, keep: bool) -> Result<()> {
    let mut info = MatchInfo::load(video).ok_or_else(|| anyhow::anyhow!("找不到這場錄影的事件檔"))?;
    info.keep = keep;
    info.save(video)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn recording(dir: &Path, stem: &str, mb: usize, keep: bool) {
        fs::write(dir.join(format!("{stem}.mp4")), vec![0u8; mb << 20]).unwrap();
        let info = format!(r#"{{"map":"m","mode":"x","started_at":"s","duration":1.0,"events":[],"keep":{keep}}}"#);
        fs::write(dir.join(format!("{stem}.events.json")), info).unwrap();
    }

    #[test]
    fn deletes_oldest_unkept_until_within_limit() {
        let dir = std::env::temp_dir().join(format!("ownclips-storage-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        recording(&dir, "2026-01-01 10-00-00", 3, false);
        recording(&dir, "2026-01-02 10-00-00", 3, true); // kept
        recording(&dir, "2026-01-03 10-00-00", 3, false);
        recording(&dir, "2026-01-04 10-00-00", 3, false);
        recording(&dir, "2026-01-05 10-00-00", 3, false);
        fs::write(dir.join("2026-01-05 10-00-00.mkv"), b"x").unwrap(); // still muxing
        fs::write(dir.join("unrelated.mp4"), vec![0u8; 5 << 20]).unwrap(); // no events file

        let names = |p: &Plan| p.delete.iter().map(|r| r.video.file_stem().unwrap().to_string_lossy().into_owned()).collect::<Vec<_>>();
        // Counted: 01..04 (12 MB). A 7 MB limit needs 5 MB gone: 01, then 03.
        let p = plan(&dir, Some(7 << 20));
        assert_eq!(p.used >> 20, 12);
        assert_eq!(names(&p), ["2026-01-01 10-00-00", "2026-01-03 10-00-00"]);
        assert!(plan(&dir, Some(13 << 20)).delete.is_empty());
        assert!(plan(&dir, None).delete.is_empty());
        // Even a tiny limit never takes the kept one.
        assert_eq!(names(&plan(&dir, Some(0))), ["2026-01-01 10-00-00", "2026-01-03 10-00-00", "2026-01-04 10-00-00"]);
        fs::remove_dir_all(&dir).unwrap();
    }
}
