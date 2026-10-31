//! A command run on the GTK main loop.

use std::ffi::OsStr;

use gtk::gio;

pub async fn text(argv: &[&str]) -> Result<String, String> {
    let owned: Vec<&OsStr> = argv.iter().map(OsStr::new).collect();
    let proc = gio::Subprocess::newv(
        &owned,
        gio::SubprocessFlags::STDOUT_PIPE | gio::SubprocessFlags::STDERR_PIPE,
    )
    .map_err(|e| e.to_string())?;
    let (out, err) = proc
        .communicate_utf8_future(None)
        .await
        .map_err(|e| e.to_string())?;
    if proc.exit_status() != 0 {
        let err = err.unwrap_or_default();
        let err = err.trim();
        if err.is_empty() {
            return Err(format!("exit {}", proc.exit_status()));
        }
        return Err(err.to_owned());
    }
    Ok(out.unwrap_or_default().to_string())
}

/// Start a command and do not wait. The Applications service and Flatpak
/// keep their own progress windows.
pub fn spawn(argv: &[&str]) -> Result<(), String> {
    let owned: Vec<&OsStr> = argv.iter().map(OsStr::new).collect();
    gio::Subprocess::newv(&owned, gio::SubprocessFlags::NONE).map_err(|e| e.to_string())?;
    Ok(())
}
