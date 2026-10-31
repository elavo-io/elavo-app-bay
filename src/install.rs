//! Install by handing a flatpakref or an AppImage to the Applications
//! service, which already shows progress and asks what to do afterwards.

use std::path::Path;

use crate::http::{self, Http};
use crate::model::AppId;
use crate::process;

pub fn flathub_ref(id: &str, title: &str) -> String {
    format!(
        "[Flatpak Ref]\nName={id}\nBranch=stable\nTitle={title}\nUrl=https://dl.flathub.org/repo/\nRuntimeRepo=https://dl.flathub.org/repo/flathub.flatpakrepo\nIsRuntime=false\n"
    )
}

pub async fn install_flathub(id: &str, title: &str) -> Result<(), String> {
    let path = http::downloads_dir().join(format!("{id}.flatpakref"));
    std::fs::write(&path, flathub_ref(id, title)).map_err(|e| e.to_string())?;
    open_path(&path)
}

pub async fn install_download(http: &Http, url: &str, file_name: &str) -> Result<(), String> {
    let bytes = http.get(url, 0).await?;
    let path = http::downloads_dir().join(safe_name(file_name));
    std::fs::write(&path, bytes).map_err(|e| e.to_string())?;
    if path.extension().and_then(|e| e.to_str()) == Some("AppImage") {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755));
        }
    }
    open_path(&path)
}

pub fn open_path(path: &Path) -> Result<(), String> {
    process::spawn(&["gio", "open", &path.to_string_lossy()])
}

pub fn open_app(id: &AppId, image_path: Option<&Path>) -> Result<(), String> {
    match id {
        AppId::Flathub(fid) | AppId::FlatpakRef(fid) => process::spawn(&["flatpak", "run", fid]),
        AppId::AppImage(_) => {
            let Some(path) = image_path else {
                return Err("not installed".into());
            };
            let desktop = path.join("launch.desktop");
            if desktop.is_file() {
                process::spawn(&["gio", "launch", &desktop.to_string_lossy()])
            } else {
                Err("no launcher".into())
            }
        }
    }
}

pub async fn update_flatpak(id: Option<&str>) -> Result<(), String> {
    let mut args = vec!["flatpak", "update", "--noninteractive", "--assumeyes"];
    if let Some(id) = id {
        args.push(id);
    }
    process::text(&args).await.map(|_| ())
}

pub async fn uninstall_flatpak(id: &str, scope: &str) -> Result<(), String> {
    let flag = if scope == "user" {
        "--user"
    } else {
        "--system"
    };
    process::text(&[
        "flatpak",
        "uninstall",
        flag,
        "--noninteractive",
        "--assumeyes",
        id,
    ])
    .await
    .map(|_| ())
}

pub fn uninstall_image(path: &Path) -> Result<(), String> {
    std::fs::remove_dir_all(path).map_err(|e| e.to_string())
}

pub fn download_name(id: &AppId, url: &str) -> String {
    let from_url = url.rsplit('/').next().unwrap_or("");
    if from_url.contains('.') {
        return from_url.to_owned();
    }
    match id {
        AppId::AppImage(name) => format!("{name}.AppImage"),
        AppId::FlatpakRef(name) | AppId::Flathub(name) => format!("{name}.flatpakref"),
    }
}

fn safe_name(name: &str) -> String {
    let name = name.rsplit('/').next().unwrap_or(name);
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if cleaned.is_empty() {
        "download".into()
    } else {
        cleaned
    }
}
