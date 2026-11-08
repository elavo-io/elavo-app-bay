//! Install and update from App Bay. Flatpak is run here, with its percentage
//! lines, instead of handing a file to the Applications window. AppImages
//! land in the user's Applications folder in the same bundle shape.

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::http::Http;
use crate::model::AppId;
use crate::process::{self, CommandResult};
use crate::task::{self, Phase, Provider};

pub type Report<'a> = &'a mut dyn FnMut(Phase, Option<f64>);

pub fn flathub_ref(id: &str, title: &str) -> String {
    task::flathub_ref(id, title)
}

pub async fn default_install_scope() -> String {
    let text = process::text(&["flatpak", "remotes", "--system", "--columns=name"]).await;
    let system = text
        .as_deref()
        .map(task::remotes_include_flathub)
        .unwrap_or(false);
    task::install_scope(None, system)
}

pub async fn install_flathub(
    id: &str,
    title: &str,
    scope: &str,
    progress: Report<'_>,
) -> Result<(), String> {
    let path = crate::http::downloads_dir().join(format!("{id}.flatpakref"));
    std::fs::write(&path, flathub_ref(id, title)).map_err(|e| e.to_string())?;
    let args = task::flatpak_install_file_args(scope, &path)?;
    let result = run_flatpak(&args, "dl.flathub.org", progress).await;
    let _ = std::fs::remove_file(&path);
    result
}

pub async fn install_flatpak_file(
    path: &Path,
    scope: &str,
    progress: Report<'_>,
) -> Result<(), String> {
    let host = "the Flatpak server";
    let args = task::flatpak_install_file_args(scope, path)?;
    run_flatpak(&args, host, progress).await
}

pub async fn update_flatpak(id: &str, scope: &str, progress: Report<'_>) -> Result<(), String> {
    let args = task::flatpak_update_args(id, scope)?;
    run_flatpak(&args, "the Flatpak remote", progress).await
}

pub async fn update_installation(scope: &str, progress: Report<'_>) -> Result<(), String> {
    let args = task::flatpak_update_all_args(scope)?;
    run_flatpak(&args, "the Flatpak remote", progress).await
}

async fn run_flatpak(args: &[String], host: &str, progress: Report<'_>) -> Result<(), String> {
    let result = drive(args, progress, true).await?;
    if result.code == 0 || task::already_installed(&result.stderr) {
        return Ok(());
    }
    Err(task::explain_flatpak(&result.stderr, host))
}

/// `wait` switches a quiet command to the waiting phase after two seconds.
async fn drive(args: &[String], progress: Report<'_>, wait: bool) -> Result<CommandResult, String> {
    progress(Phase::Preparing, None);
    let saw = Cell::new(false);
    let announced = Cell::new(false);
    let progress = RefCell::new(progress);
    process::stream(
        args,
        |line| {
            if let Some(parsed) = task::read_flatpak_line(line) {
                if parsed.fraction.is_some() {
                    saw.set(true);
                }
                let phase = parsed.phase.unwrap_or(Phase::Installing);
                progress.borrow_mut()(phase, parsed.fraction);
            }
        },
        |elapsed| {
            if wait && !saw.get() && !announced.get() && elapsed >= Duration::from_secs(2) {
                announced.set(true);
                progress.borrow_mut()(Phase::Waiting, None);
            }
        },
    )
    .await
}

pub async fn download_to(
    http: &Http,
    url: &str,
    dest: &Path,
    progress: Report<'_>,
) -> Result<(), String> {
    if !task::is_http(url) {
        return Err("The download address is not a web address.".into());
    }
    progress(Phase::Downloading, None);
    let saw = Cell::new(false);
    http.download_file(url, dest, |got, total| {
        saw.set(true);
        let fraction = total.and_then(|total| (total > 0).then(|| got as f64 / total as f64));
        progress(Phase::Downloading, fraction);
    })
    .await?;
    if !saw.get() {
        progress(Phase::Downloading, None);
    }
    Ok(())
}

pub async fn update_appimage(
    bundle: &Path,
    source_name: &str,
    provider: &Provider,
    http: &Http,
    progress: Report<'_>,
) -> Result<(), String> {
    let version = provider.version().to_owned();
    match provider {
        Provider::WholeFile { url, .. } => {
            let staged = crate::http::downloads_dir()
                .join(format!("update-{}", task::safe_file_name(source_name)));
            let downloaded = download_to(http, url, &staged, progress).await;
            if let Err(err) = downloaded {
                let _ = std::fs::remove_file(&staged);
                return Err(err);
            }
            progress(Phase::Installing, None);
            let replaced = task::replace_appimage(bundle, source_name, &staged, &version);
            let _ = std::fs::remove_file(&staged);
            replaced
        }
        Provider::Zsync { url, .. } => {
            zsync_into(bundle, source_name, url, &version, progress).await
        }
        Provider::GitHub { file, .. } => {
            let url = match task::github_zsync_url(provider) {
                Ok(url) => url,
                Err(_) => resolve_latest(http, provider, file).await?,
            };
            zsync_into(bundle, source_name, &url, &version, progress).await
        }
    }
}

async fn resolve_latest(http: &Http, provider: &Provider, file: &str) -> Result<String, String> {
    let Provider::GitHub { user, repo, .. } = provider else {
        return Err(task::unsupported_image_message().into());
    };
    let api = format!("https://api.github.com/repos/{user}/{repo}/releases/latest");
    let bytes = http.get(&api, 0).await?;
    let json = String::from_utf8_lossy(&bytes);
    task::github_asset_url(&json, file)
}

async fn zsync_into(
    bundle: &Path,
    source_name: &str,
    url: &str,
    version: &str,
    progress: Report<'_>,
) -> Result<(), String> {
    if !command_exists("zsync") {
        return Err(task::missing_zsync_message().into());
    }
    let basis = bundle.join(source_name);
    if !basis.is_file() {
        return Err("The installed AppImage is missing, so it was left unchanged.".into());
    }
    let output =
        crate::http::downloads_dir().join(format!("zsync-{}", task::safe_file_name(source_name)));
    let _ = std::fs::remove_file(&output);
    progress(Phase::Downloading, None);
    let args = task::zsync_args(&basis, &output, url);
    let result = drive(&args, progress, true).await;
    let result = match result {
        Ok(result) if result.code == 0 && output.is_file() => {
            progress(Phase::Installing, None);
            task::replace_appimage(bundle, source_name, &output, version)
        }
        Ok(result) => {
            let _ = std::fs::remove_file(&output);
            Err(if result.stderr.trim().is_empty() {
                "The zsync update failed. The installed copy was left unchanged.".into()
            } else {
                format!(
                    "{}. The installed copy was left unchanged.",
                    result
                        .stderr
                        .lines()
                        .last()
                        .unwrap_or("zsync failed")
                        .trim()
                )
            })
        }
        Err(err) => {
            let _ = std::fs::remove_file(&output);
            Err(format!("{err} The installed copy was left unchanged."))
        }
    };
    let _ = std::fs::remove_file(&output);
    result
}

pub async fn install_appimage(
    http: &Http,
    url: &str,
    name: &str,
    id: &str,
    version: &str,
    progress: Report<'_>,
) -> Result<PathBuf, String> {
    let file_name = task::safe_file_name(&download_name(&AppId::AppImage(id.to_owned()), url));
    let staged = crate::http::downloads_dir().join(format!("new-{file_name}"));
    if let Err(err) = download_to(http, url, &staged, progress).await {
        let _ = std::fs::remove_file(&staged);
        return Err(err);
    }
    progress(Phase::Installing, None);
    let root = user_applications();
    let _ = std::fs::create_dir_all(&root);
    let placed = task::place_new_appimage(&root, name, id, version, &staged);
    let _ = std::fs::remove_file(&staged);
    placed
}

pub fn user_applications() -> PathBuf {
    gtk::glib::home_dir().join("Applications")
}

fn command_exists(name: &str) -> bool {
    std::process::Command::new(name)
        .arg("-h")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok()
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
        return task::safe_file_name(from_url);
    }
    match id {
        AppId::AppImage(name) => format!("{}.AppImage", task::safe_file_name(name)),
        AppId::FlatpakRef(name) | AppId::Flathub(name) => {
            format!("{}.flatpakref", task::safe_file_name(name))
        }
    }
}
