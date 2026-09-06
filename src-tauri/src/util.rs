use std::path::PathBuf;

use chrono::Utc;

/// Milliseconds since the Unix epoch.
pub fn now_ms() -> i64 {
    Utc::now().timestamp_millis()
}

/// Where the app stores its writable data (sqlite DB, tile cache, the
/// one-off Zadig download, etc). Normally the OS-standard per-user app-data
/// directory — but if a `portable.txt` marker file sits next to the running
/// executable, use a `data` folder in that same directory instead, so a zip
/// extracted to a USB stick (or anywhere else) leaves zero footprint on the
/// host machine's normal per-user profile. Opt-in only (the marker file has
/// to actually be there) — a normal installed copy's behavior is unchanged,
/// since `tauri build`'s installer output never plants that file itself.
///
/// User-requested exports (ATC recordings, logbook CSV) deliberately don't
/// go through this — those go to the real Downloads/Documents folder
/// regardless of portable mode, same as any other app's "Save As".
pub fn resolve_data_dir(handle: &tauri::AppHandle) -> std::io::Result<PathBuf> {
    use tauri::Manager;

    if let Some(dir) = portable_data_dir() {
        std::fs::create_dir_all(&dir)?;
        return Ok(dir);
    }
    let dir = handle
        .path()
        .app_data_dir()
        .map_err(|e| std::io::Error::other(e.to_string()))?;
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// The portable-mode data directory (`data/` next to the executable), or
/// `None` when the `portable.txt` marker file isn't present — i.e. a normal
/// installed copy. See `resolve_data_dir` for the rationale.
fn portable_data_dir() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let exe_dir = exe.parent()?;
    exe_dir
        .join("portable.txt")
        .exists()
        .then(|| exe_dir.join("data"))
}

/// Whether this copy is running in portable mode — used by the update check
/// to point the user at the matching download (portable zip vs. installer).
pub fn is_portable() -> bool {
    portable_data_dir().is_some()
}
