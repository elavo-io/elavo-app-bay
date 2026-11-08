//! Decisions for Get and Update that can be checked without installing
//! anything: Flatpak command lines, progress lines, and whether an AppImage
//! has a real update provider.

use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

/// What the pill is doing. `Waiting` is a running command that has not
/// reported a percentage yet, which is how a password prompt looks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Preparing,
    Waiting,
    Downloading,
    Installing,
    Updating,
    Finishing,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FlatpakLine {
    pub phase: Option<Phase>,
    pub fraction: Option<f64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum JobKind {
    Flatpak,
    Image,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Job {
    Working {
        phase: Phase,
        fraction: Option<f64>,
        kind: JobKind,
    },
    Failed {
        message: String,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BeginError {
    /// This app already has a command running.
    Duplicate,
    /// Flatpak runs one transaction at a time.
    FlatpakBusy,
}

#[derive(Clone, Debug, Default)]
pub struct Jobs {
    map: HashMap<String, Job>,
}

impl Jobs {
    pub fn get(&self, key: &str) -> Option<&Job> {
        self.map.get(key)
    }

    pub fn working(&self, key: &str) -> bool {
        matches!(self.map.get(key), Some(Job::Working { .. }))
    }

    pub fn flatpak_busy(&self) -> bool {
        self.map.values().any(|job| {
            matches!(
                job,
                Job::Working {
                    kind: JobKind::Flatpak,
                    ..
                }
            )
        })
    }

    /// Starts `key` unless that app, or another Flatpak transaction, is
    /// already running. A failed job can be started again.
    pub fn try_begin(&mut self, key: &str, kind: JobKind) -> Result<(), BeginError> {
        if self.working(key) {
            return Err(BeginError::Duplicate);
        }
        if kind == JobKind::Flatpak && self.flatpak_busy() {
            return Err(BeginError::FlatpakBusy);
        }
        self.map.insert(
            key.to_owned(),
            Job::Working {
                phase: Phase::Preparing,
                fraction: None,
                kind,
            },
        );
        Ok(())
    }

    pub fn progress(&mut self, key: &str, phase: Phase, fraction: Option<f64>) {
        let Some(Job::Working {
            fraction: slot,
            phase: phase_slot,
            ..
        }) = self.map.get_mut(key)
        else {
            return;
        };
        *phase_slot = phase;
        if fraction.is_some() {
            *slot = fraction;
        }
    }

    pub fn fail(&mut self, key: &str, message: impl Into<String>) {
        self.map.insert(
            key.to_owned(),
            Job::Failed {
                message: message.into(),
            },
        );
    }

    pub fn clear(&mut self, key: &str) -> bool {
        self.map.remove(key).is_some()
    }

    /// Shows `key` as running without taking the Flatpak lock. Update All
    /// uses this so each app's pill moves while one transaction runs.
    pub fn follow(&mut self, key: &str, kind: JobKind) {
        if self.working(key) {
            return;
        }
        self.map.insert(
            key.to_owned(),
            Job::Working {
                phase: Phase::Preparing,
                fraction: None,
                kind,
            },
        );
    }
}

/// Scope flag for one Flatpak installation. Empty and `system` are the
/// system installation. Anything else is a named installation, not a second
/// copy of `--system`.
pub fn scope_flag(scope: &str) -> Result<String, String> {
    let scope = scope.trim();
    if scope.is_empty() || scope == "system" {
        return Ok("--system".into());
    }
    if scope == "user" {
        return Ok("--user".into());
    }
    if scope_token(scope) {
        return Ok(format!("--installation={scope}"));
    }
    Err(format!(
        "Flatpak installation \"{scope}\" is not a name this command can use."
    ))
}

fn scope_token(scope: &str) -> bool {
    !scope.is_empty()
        && !scope.starts_with('-')
        && scope
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
}

pub fn flatpak_id_ok(id: &str) -> bool {
    id.contains('.')
        && !id.starts_with('-')
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-')
}

fn require_id(id: &str) -> Result<(), String> {
    if flatpak_id_ok(id) {
        Ok(())
    } else {
        Err(format!("\"{id}\" is not a Flatpak application id."))
    }
}

/// `flatpak update` for one app. `-y` answers questions. The scope flag is
/// required: omitting it updates the system installation only. No
/// `--noninteractive`, because that hides the percentage lines.
pub fn flatpak_update_args(id: &str, scope: &str) -> Result<Vec<String>, String> {
    require_id(id)?;
    Ok(vec![
        "flatpak".into(),
        "update".into(),
        "-y".into(),
        scope_flag(scope)?,
        "--app".into(),
        "--".into(),
        id.into(),
    ])
}

pub fn flatpak_update_all_args(scope: &str) -> Result<Vec<String>, String> {
    Ok(vec![
        "flatpak".into(),
        "update".into(),
        "-y".into(),
        scope_flag(scope)?,
    ])
}

pub fn flatpak_install_file_args(scope: &str, file: &Path) -> Result<Vec<String>, String> {
    Ok(vec![
        "flatpak".into(),
        "install".into(),
        "-y".into(),
        scope_flag(scope)?,
        "--from".into(),
        file.to_string_lossy().into_owned(),
    ])
}

/// Where a new Flatpak goes. An app that is already installed keeps that
/// installation. Otherwise the system Flathub remote matches the shared
/// Applications folder; without it, the per-user installation is the one
/// that does not need an administrator.
pub fn install_scope(already: Option<&str>, system_flathub: bool) -> String {
    if let Some(scope) = already.map(str::trim).filter(|s| !s.is_empty()) {
        return scope.to_owned();
    }
    if system_flathub {
        "system".into()
    } else {
        "user".into()
    }
}

/// One line of `flatpak install` or `flatpak update`. The fraction counts
/// finished steps plus the current step's percentage, from 0 to 1.
/// `Installing app/org.x/x86_64/stable` names the phase and has no percentage.
pub fn read_flatpak_line(line: &str) -> Option<FlatpakLine> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }
    let mut words = line.split_whitespace();
    let phase = match words.next() {
        Some("Installing") => Some(Phase::Installing),
        Some("Updating") => Some(Phase::Updating),
        Some("Downloading") => Some(Phase::Downloading),
        _ => None,
    }?;
    let step = words.next().unwrap_or("").trim_end_matches(['…', '.']);
    let fraction = step.split_once('/').and_then(|(done, total)| {
        let done: f64 = done.parse().ok()?;
        let total: f64 = total.parse().ok()?;
        if done < 1.0 || total < done {
            return None;
        }
        let percent = line
            .split_whitespace()
            .rev()
            .find_map(|w| w.strip_suffix('%')?.parse::<f64>().ok())
            .unwrap_or(0.0);
        Some(((done - 1.0 + percent / 100.0) / total).clamp(0.0, 1.0))
    });
    if fraction.is_none() && !line.contains('/') {
        // A ref line still tells us the phase. A sentence that merely
        // starts with the word and has no step does not.
        let rest = line.split_whitespace().nth(1).unwrap_or("");
        if !rest.starts_with("app/") && !rest.starts_with("runtime/") {
            return None;
        }
    }
    Some(FlatpakLine {
        phase: Some(phase),
        fraction,
    })
}

pub fn already_installed(stderr: &str) -> bool {
    stderr.to_lowercase().contains("already installed")
}

pub fn explain_flatpak(stderr: &str, host: &str) -> String {
    let lower = stderr.to_lowercase();
    let says = |words: &[&str]| words.iter().any(|w| lower.contains(w));
    if says(&[
        "could not resolve",
        "couldn't resolve",
        "name resolution",
        "network is unreachable",
        "no route to host",
        "connection refused",
    ]) {
        format!("Couldn't reach {host}. Check the network connection, then try again.")
    } else if says(&["timeout was reached", "timed out"]) {
        format!("{host} took too long to respond. Check the network connection, then try again.")
    } else if says(&["no space left"]) {
        "There isn't enough free space on this computer.".to_owned()
    } else if says(&[
        "not authorized",
        "not allowed for user",
        "authentication",
        "polkit",
        "interactive authentication",
    ]) {
        "Installing or updating for every account needs an administrator's password.".to_owned()
    } else if says(&["gpg", "signature"]) {
        "The download couldn't be verified, so nothing was changed.".to_owned()
    } else {
        stderr
            .lines()
            .rev()
            .map(str::trim)
            .find(|l| !l.is_empty())
            .map(|l| l.strip_prefix("error: ").unwrap_or(l).to_owned())
            .filter(|l| !l.is_empty())
            .unwrap_or_else(|| "Flatpak stopped without saying why.".to_owned())
    }
}

/// After Flatpak exits successfully, keep the remote's list when that
/// refresh worked. When it did not, drop the app we just updated so a stale
/// row (the installed version and the listed version are the same text)
/// cannot keep offering Update.
pub fn settle_updates(
    current: &HashMap<String, String>,
    id: Option<&str>,
    remote: Result<HashMap<String, String>, ()>,
) -> HashMap<String, String> {
    match remote {
        Ok(list) => list,
        Err(()) => {
            let mut kept = current.clone();
            match id {
                Some(id) => {
                    kept.remove(id);
                }
                None => kept.clear(),
            }
            kept
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Provider {
    /// Download this AppImage and replace the installed file only after it
    /// is complete.
    WholeFile {
        url: String,
        version: String,
    },
    Zsync {
        url: String,
        version: String,
    },
    GitHub {
        user: String,
        repo: String,
        release: String,
        file: String,
        version: String,
    },
}

impl Provider {
    pub fn version(&self) -> &str {
        match self {
            Self::WholeFile { version, .. }
            | Self::Zsync { version, .. }
            | Self::GitHub { version, .. } => version,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImagePlan {
    NotInstalled,
    Current,
    /// The listing is newer, but nothing describes how to fetch the update.
    Unsupported,
    Ready(Provider),
}

pub fn unsupported_image_message() -> &'static str {
    "This AppImage does not publish an update. The installed copy was left unchanged."
}

pub fn missing_zsync_message() -> &'static str {
    "This AppImage publishes a zsync update, and the zsync tool is not installed. The installed copy was left unchanged."
}

/// An AppImage update is ready only when the catalog version differs and a
/// provider exists. Embedded update information wins over re-downloading the
/// catalog file. A different version string alone is not a provider.
pub fn image_plan(
    installed_version: &str,
    catalog_version: &str,
    download: &str,
    catalog_update: &str,
    embedded: &str,
) -> ImagePlan {
    if installed_version.is_empty() && catalog_version.is_empty() {
        return ImagePlan::Current;
    }
    if !catalog_version.is_empty() && installed_version == catalog_version {
        return ImagePlan::Current;
    }
    if catalog_version.is_empty() || installed_version.is_empty() {
        return ImagePlan::Current;
    }
    if let Some(provider) = provider_from_info(embedded, catalog_version) {
        return ImagePlan::Ready(provider);
    }
    if let Some(provider) = provider_from_info(catalog_update, catalog_version) {
        return ImagePlan::Ready(provider);
    }
    if is_http(catalog_update) && catalog_update.contains(".zsync") {
        return ImagePlan::Ready(Provider::Zsync {
            url: catalog_update.trim().to_owned(),
            version: catalog_version.to_owned(),
        });
    }
    if is_http(download) {
        return ImagePlan::Ready(Provider::WholeFile {
            url: download.trim().to_owned(),
            version: catalog_version.to_owned(),
        });
    }
    ImagePlan::Unsupported
}

fn provider_from_info(raw: &str, version: &str) -> Option<Provider> {
    let raw = raw.trim().trim_matches(char::from(0));
    if raw.is_empty() {
        return None;
    }
    let version = version.to_owned();
    if let Some(url) = raw.strip_prefix("zsync|") {
        let url = url.trim();
        if is_http(url) {
            return Some(Provider::Zsync {
                url: url.to_owned(),
                version,
            });
        }
        return None;
    }
    if let Some(rest) = raw.strip_prefix("gh-releases-zsync|") {
        let parts: Vec<&str> = rest.split('|').collect();
        if parts.len() == 4 && parts.iter().all(|p| github_token(p)) {
            return Some(Provider::GitHub {
                user: parts[0].to_owned(),
                repo: parts[1].to_owned(),
                release: parts[2].to_owned(),
                file: parts[3].to_owned(),
                version,
            });
        }
        return None;
    }
    None
}

fn github_token(part: &str) -> bool {
    !part.is_empty()
        && !part.contains('/')
        && !part.contains('\\')
        && !part.contains("..")
        && !part.contains(' ')
}

pub fn is_http(url: &str) -> bool {
    let url = url.trim();
    url.starts_with("https://") || url.starts_with("http://")
}

pub fn github_zsync_url(provider: &Provider) -> Result<String, String> {
    let Provider::GitHub {
        user,
        repo,
        release,
        file,
        ..
    } = provider
    else {
        return Err("Not a GitHub update.".into());
    };
    if release == "latest" {
        return Err("latest".into());
    }
    Ok(format!(
        "https://github.com/{user}/{repo}/releases/download/{release}/{file}"
    ))
}

/// The `.zsync` asset URL from a GitHub release document.
pub fn github_asset_url(json: &str, file: &str) -> Result<String, String> {
    let value: serde_json::Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
    let assets = value
        .get("assets")
        .and_then(|v| v.as_array())
        .ok_or("The GitHub release has no files.")?;
    assets
        .iter()
        .find(|a| a.get("name").and_then(|v| v.as_str()) == Some(file))
        .and_then(|a| a.get("browser_download_url").and_then(|v| v.as_str()))
        .filter(|url| is_http(url))
        .map(str::to_owned)
        .ok_or_else(|| format!("The GitHub release has no {file}."))
}

pub fn zsync_args(basis: &Path, output: &Path, url: &str) -> Vec<String> {
    vec![
        "zsync".into(),
        "-i".into(),
        basis.to_string_lossy().into_owned(),
        "-o".into(),
        output.to_string_lossy().into_owned(),
        url.to_owned(),
    ]
}

pub fn looks_like_appimage(header: &[u8]) -> bool {
    header.len() >= 11 && &header[0..4] == b"\x7fELF" && &header[8..10] == b"AI"
}

pub fn read_update_info(path: &Path) -> Option<String> {
    let mut file = std::fs::File::open(path).ok()?;
    update_info_from_reader(&mut file)
}

pub fn update_info_from_reader(file: &mut (impl Read + Seek)) -> Option<String> {
    let mut head = [0u8; 64];
    file.read_exact(&mut head).ok()?;
    if &head[0..4] != b"\x7fELF" || head[5] != 1 {
        return None;
    }
    let (shoff, shentsize, shnum, shstrndx, class) = match head[4] {
        2 => (
            u64::from_le_bytes(head[0x28..0x30].try_into().ok()?),
            u16::from_le_bytes(head[0x3a..0x3c].try_into().ok()?),
            u16::from_le_bytes(head[0x3c..0x3e].try_into().ok()?),
            u16::from_le_bytes(head[0x3e..0x40].try_into().ok()?),
            64u16,
        ),
        1 => (
            u64::from(u32::from_le_bytes(head[0x20..0x24].try_into().ok()?)),
            u16::from_le_bytes(head[0x2e..0x30].try_into().ok()?),
            u16::from_le_bytes(head[0x30..0x32].try_into().ok()?),
            u16::from_le_bytes(head[0x32..0x34].try_into().ok()?),
            32u16,
        ),
        _ => return None,
    };
    if shnum == 0 || shnum > 512 || shstrndx >= shnum || shentsize < 40 {
        return None;
    }
    file.seek(SeekFrom::Start(shoff)).ok()?;
    let mut headers = vec![0u8; usize::from(shentsize) * usize::from(shnum)];
    file.read_exact(&mut headers).ok()?;
    let (name_off, name_len, _) = section_loc(&headers, shentsize, shstrndx, class)?;
    if name_len == 0 || name_len > 65_536 {
        return None;
    }
    let mut names = vec![0u8; name_len as usize];
    file.seek(SeekFrom::Start(name_off)).ok()?;
    file.read_exact(&mut names).ok()?;
    for index in 0..shnum {
        let (off, size, name_at) = section_loc(&headers, shentsize, index, class)?;
        if cstr_at(&names, name_at) != Some(".upd_info") {
            continue;
        }
        if size == 0 || size > 4096 {
            return None;
        }
        let mut buf = vec![0u8; size as usize];
        file.seek(SeekFrom::Start(off)).ok()?;
        file.read_exact(&mut buf).ok()?;
        let text = String::from_utf8_lossy(&buf);
        let text = text.trim_matches(char::from(0)).trim();
        if text.is_empty() {
            return None;
        }
        return Some(text.to_owned());
    }
    None
}

/// `(offset, size, name_offset)` for section `index`.
fn section_loc(headers: &[u8], entsize: u16, index: u16, class: u16) -> Option<(u64, u64, u32)> {
    let start = usize::from(entsize) * usize::from(index);
    let ent = headers.get(start..start + usize::from(entsize))?;
    let name = u32::from_le_bytes(ent.get(0..4)?.try_into().ok()?);
    if class == 64 {
        let off = u64::from_le_bytes(ent.get(24..32)?.try_into().ok()?);
        let size = u64::from_le_bytes(ent.get(32..40)?.try_into().ok()?);
        Some((off, size, name))
    } else {
        let off = u64::from(u32::from_le_bytes(ent.get(16..20)?.try_into().ok()?));
        let size = u64::from(u32::from_le_bytes(ent.get(20..24)?.try_into().ok()?));
        Some((off, size, name))
    }
}

fn cstr_at(buf: &[u8], at: u32) -> Option<&str> {
    let rest = buf.get(at as usize..)?;
    let end = rest.iter().position(|b| *b == 0).unwrap_or(rest.len());
    std::str::from_utf8(&rest[..end]).ok()
}

pub fn safe_file_name(name: &str) -> String {
    let name = name.rsplit(['/', '\\']).next().unwrap_or(name);
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
    if cleaned.is_empty() || cleaned.starts_with('.') {
        "download".into()
    } else {
        cleaned
    }
}

pub fn safe_bundle_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| match c {
            '/' | '\\' | '\0' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();
    let cleaned = cleaned.trim().trim_matches('.').trim();
    if cleaned.is_empty() {
        "App".into()
    } else {
        cleaned.to_owned()
    }
}

fn appimage_header_ok(path: &Path) -> Result<(), String> {
    let mut file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut header = [0u8; 11];
    let n = file.read(&mut header).map_err(|e| e.to_string())?;
    if looks_like_appimage(&header[..n]) {
        Ok(())
    } else {
        Err("The download is not an AppImage. The installed copy was left unchanged.".into())
    }
}

/// Replaces the AppImage inside `bundle` only after `staged` looks like an
/// AppImage. A failure while recording the version puts the previous file back.
pub fn replace_appimage(
    bundle: &Path,
    source_name: &str,
    staged: &Path,
    version: &str,
) -> Result<(), String> {
    if source_name.is_empty()
        || source_name.contains('/')
        || source_name.contains('\\')
        || source_name.contains("..")
    {
        return Err("The installed AppImage has no file name to replace.".into());
    }
    if version.chars().any(|c| c == '\n' || c == '\r' || c == '\0') {
        return Err("The catalog version cannot be stored.".into());
    }
    appimage_header_ok(staged)?;
    let dest = bundle.join(source_name);
    let partial = bundle.join(format!("{source_name}.partial"));
    let backup = bundle.join(format!("{source_name}.bak"));
    std::fs::copy(staged, &partial).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&partial, std::fs::Permissions::from_mode(0o755));
    }
    let had_dest = dest.is_file();
    if had_dest {
        let _ = std::fs::remove_file(&backup);
        if let Err(err) = std::fs::rename(&dest, &backup) {
            let _ = std::fs::remove_file(&partial);
            return Err(format!("{err}. The installed copy was left unchanged."));
        }
    }
    if let Err(err) = std::fs::rename(&partial, &dest) {
        if had_dest {
            let _ = std::fs::rename(&backup, &dest);
        }
        let _ = std::fs::remove_file(&partial);
        return Err(format!("{err}. The installed copy was left unchanged."));
    }
    if let Err(err) = write_entry_version(bundle, version) {
        let _ = std::fs::remove_file(&dest);
        if had_dest {
            let _ = std::fs::rename(&backup, &dest);
        }
        let _ = std::fs::remove_file(&partial);
        return Err(format!("{err} The installed copy was left unchanged."));
    }
    let _ = std::fs::remove_file(&backup);
    let _ = std::fs::remove_file(&partial);
    Ok(())
}

pub fn write_entry_version(bundle: &Path, version: &str) -> Result<(), String> {
    let path = bundle.join("entry.json");
    let text = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let mut value: serde_json::Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    let obj = value.as_object_mut().ok_or("entry.json is not a record")?;
    obj.insert(
        "version".into(),
        serde_json::Value::String(version.to_owned()),
    );
    let pretty = serde_json::to_string_pretty(&value).map_err(|e| e.to_string())? + "\n";
    let tmp = bundle.join("entry.json.tmp");
    std::fs::write(&tmp, pretty.as_bytes()).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &path).map_err(|e| e.to_string())
}

/// A new AppImage bundle in `root`, in the Applications `*.eapp` shape.
/// Failure removes the directory this function created.
pub fn place_new_appimage(
    root: &Path,
    name: &str,
    id: &str,
    version: &str,
    staged: &Path,
) -> Result<PathBuf, String> {
    if id.trim().is_empty() || id.contains('/') || id.contains("..") {
        return Err("This AppImage has no id.".into());
    }
    appimage_header_ok(staged)?;
    let file_name = staged
        .file_name()
        .map(|n| safe_file_name(&n.to_string_lossy()))
        .filter(|n| n.ends_with(".AppImage") || n.ends_with(".appimage"))
        .unwrap_or_else(|| format!("{}.AppImage", safe_file_name(id)));
    let dir_name = safe_bundle_name(name);
    let mut path = root.join(format!("{dir_name}.eapp"));
    let mut n = 2;
    while path.exists() {
        path = root.join(format!("{dir_name} {n}.eapp"));
        n += 1;
    }
    if let Err(err) = write_new_bundle(&path, name, id, version, staged, &file_name) {
        let _ = std::fs::remove_dir_all(&path);
        return Err(err);
    }
    Ok(path)
}

fn write_new_bundle(
    path: &Path,
    name: &str,
    id: &str,
    version: &str,
    staged: &Path,
    file_name: &str,
) -> Result<(), String> {
    std::fs::create_dir_all(path).map_err(|e| e.to_string())?;
    let dest = path.join(file_name);
    std::fs::copy(staged, &dest).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| e.to_string())?;
    }
    let entry = serde_json::json!({
        "kind": "appimage",
        "id": id,
        "name": name,
        "scope": "user",
        "origin": "",
        "source": file_name,
        "locked": false,
        "version": version,
    });
    std::fs::write(
        path.join("entry.json"),
        serde_json::to_string_pretty(&entry).map_err(|e| e.to_string())? + "\n",
    )
    .map_err(|e| e.to_string())?;
    let run = run_script(file_name);
    std::fs::write(path.join("run"), run.as_bytes()).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path.join("run"), std::fs::Permissions::from_mode(0o755))
            .map_err(|e| e.to_string())?;
    }
    let desktop = launch_desktop(name, &path.join("run"));
    std::fs::write(path.join("launch.desktop"), desktop.as_bytes()).map_err(|e| e.to_string())?;
    Ok(())
}

fn run_script(file_name: &str) -> String {
    let quoted = format!("'{}'", file_name.replace('\'', "'\\''"));
    format!(
        "#!/bin/sh\n\
         # Starts the AppImage from this bundle.\n\
         cd \"$(dirname \"$0\")\" || exit 1\n\
         if [ -z \"${{GTK_THEME:-}}\" ] && [ -d /usr/share/themes/Elavo ]; then\n\
         \x20   case \"$(gsettings get org.gnome.desktop.interface color-scheme 2>/dev/null)\" in\n\
         \x20       *dark*) export GTK_THEME=Elavo:dark ;;\n\
         \x20       *) export GTK_THEME=Elavo ;;\n\
         \x20   esac\n\
         fi\n\
         export XDG_DATA_DIRS=\"${{XDG_DATA_DIRS:-/usr/local/share:/usr/share}}\"\n\
         exec ./{quoted} \"$@\"\n"
    )
}

fn launch_desktop(name: &str, run: &Path) -> String {
    let exec = run.to_string_lossy().replace('"', "\\\"");
    let name = name.replace('\n', " ");
    format!("[Desktop Entry]\nType=Application\nName={name}\nExec=\"{exec}\"\n")
}

pub fn flathub_ref(id: &str, title: &str) -> String {
    let title: String = title
        .chars()
        .map(|c| if c == '\n' || c == '\r' { ' ' } else { c })
        .collect();
    format!(
        "[Flatpak Ref]\nName={id}\nBranch=stable\nTitle={title}\nUrl=https://dl.flathub.org/repo/\nRuntimeRepo=https://dl.flathub.org/repo/flathub.flatpakrepo\nIsRuntime=false\n"
    )
}

/// True when `text` lists a remote named `flathub`, one name per line.
pub fn remotes_include_flathub(text: &str) -> bool {
    text.lines().any(|line| line.trim() == "flathub")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn gimp_update_targets_the_system_install_and_prints_progress() {
        let args = flatpak_update_args("org.gimp.GIMP", "system").unwrap();
        assert_eq!(
            args,
            [
                "flatpak",
                "update",
                "-y",
                "--system",
                "--app",
                "--",
                "org.gimp.GIMP"
            ]
        );
        assert!(!args.iter().any(|a| a == "--noninteractive"));
    }

    #[test]
    fn user_and_named_installations_are_not_the_system_default() {
        assert_eq!(
            flatpak_update_args("org.gimp.GIMP", "user").unwrap()[3],
            "--user"
        );
        assert_eq!(
            flatpak_update_all_args("studio").unwrap()[3],
            "--installation=studio"
        );
        assert!(scope_flag("--user").is_err());
        assert!(flatpak_update_args("-o", "user").is_err());
        assert_eq!(install_scope(Some("user"), true), "user");
        assert_eq!(install_scope(None, true), "system");
        assert_eq!(install_scope(None, false), "user");
    }

    #[test]
    fn progress_lines_match_flatpak_output() {
        assert_eq!(
            read_flatpak_line("Installing 1/2…"),
            Some(FlatpakLine {
                phase: Some(Phase::Installing),
                fraction: Some(0.0),
            })
        );
        let mid = read_flatpak_line("Installing 1/2… ████████▏           50%  31.7 kB/s").unwrap();
        assert_eq!(mid.phase, Some(Phase::Installing));
        assert!((mid.fraction.unwrap() - 0.25).abs() < 0.001);
        assert_eq!(
            read_flatpak_line("Installing 2/2… ████ 100%")
                .unwrap()
                .fraction,
            Some(1.0)
        );
        assert_eq!(
            read_flatpak_line("Updating 1/1… 10%").unwrap().phase,
            Some(Phase::Updating)
        );
        assert_eq!(
            read_flatpak_line("Installing app/org.gimp.GIMP/x86_64/stable")
                .unwrap()
                .fraction,
            None
        );
        assert_eq!(
            read_flatpak_line("Configuring https://x as new remote"),
            None
        );
        assert_eq!(read_flatpak_line("Nothing to do."), None);
    }

    #[test]
    fn repeated_clicks_do_not_start_a_second_flatpak() {
        let mut jobs = Jobs::default();
        assert!(jobs
            .try_begin("flathub:org.gimp.GIMP", JobKind::Flatpak)
            .is_ok());
        assert_eq!(
            jobs.try_begin("flathub:org.gimp.GIMP", JobKind::Flatpak),
            Err(BeginError::Duplicate)
        );
        assert_eq!(
            jobs.try_begin("flathub:org.inkscape.Inkscape", JobKind::Flatpak),
            Err(BeginError::FlatpakBusy)
        );
        jobs.fail("flathub:org.gimp.GIMP", "network");
        assert!(jobs
            .try_begin("flathub:org.inkscape.Inkscape", JobKind::Flatpak)
            .is_ok());
    }

    #[test]
    fn a_failed_refresh_drops_the_stale_gimp_update() {
        let mut current = HashMap::new();
        current.insert("org.gimp.GIMP".into(), "3.2.8".into());
        current.insert("org.inkscape.Inkscape".into(), "1.4".into());
        let kept = settle_updates(&current, Some("org.gimp.GIMP"), Err(()));
        assert!(!kept.contains_key("org.gimp.GIMP"));
        assert_eq!(kept["org.inkscape.Inkscape"], "1.4");
        let mut remote = HashMap::new();
        remote.insert("org.inkscape.Inkscape".into(), "1.4".into());
        let fresh = settle_updates(&current, Some("org.gimp.GIMP"), Ok(remote));
        assert!(!fresh.contains_key("org.gimp.GIMP"));
    }

    #[test]
    fn failures_stay_in_plain_words() {
        let offline = "error: While pulling app/org.x: Could not resolve hostname";
        assert!(explain_flatpak(offline, "dl.flathub.org").contains("dl.flathub.org"));
        assert!(
            explain_flatpak("error: Interactive authentication required", "h")
                .contains("administrator")
        );
        assert!(already_installed(
            "error: org.x/x86_64/stable already installed"
        ));
    }

    #[test]
    fn appimage_update_needs_a_provider() {
        assert_eq!(image_plan("1", "2", "", "", ""), ImagePlan::Unsupported);
        assert_eq!(
            image_plan("1", "1", "https://example.com/a.AppImage", "", ""),
            ImagePlan::Current
        );
        assert_eq!(
            image_plan("", "2", "https://example.com/a.AppImage", "", ""),
            ImagePlan::Current
        );
        assert!(matches!(
            image_plan("1", "2", "https://example.com/a.AppImage", "", ""),
            ImagePlan::Ready(Provider::WholeFile { .. })
        ));
        let embedded = image_plan(
            "1",
            "2",
            "https://example.com/a.AppImage",
            "",
            "zsync|https://example.com/a.zsync",
        );
        assert_eq!(
            embedded,
            ImagePlan::Ready(Provider::Zsync {
                url: "https://example.com/a.zsync".into(),
                version: "2".into(),
            })
        );
    }

    #[test]
    fn update_information_is_read_from_the_elf_section() {
        let info = "zsync|https://example.com/a.zsync";
        let bytes = elf_with_upd_info(info);
        let mut cursor = Cursor::new(bytes);
        assert_eq!(update_info_from_reader(&mut cursor).as_deref(), Some(info));
        assert!(matches!(
            image_plan("1", "2", "", "", info),
            ImagePlan::Ready(Provider::Zsync { .. })
        ));
        assert_eq!(
            github_zsync_url(&Provider::GitHub {
                user: "me".into(),
                repo: "app".into(),
                release: "v1".into(),
                file: "App.zsync".into(),
                version: "1".into(),
            })
            .unwrap(),
            "https://github.com/me/app/releases/download/v1/App.zsync"
        );
        let json = r#"{"assets":[{"name":"App.zsync","browser_download_url":"https://github.com/me/app/releases/download/v1/App.zsync"}]}"#;
        assert!(github_asset_url(json, "App.zsync")
            .unwrap()
            .ends_with("App.zsync"));
    }

    #[test]
    fn a_bad_download_does_not_replace_the_installed_appimage() {
        let dir = std::env::temp_dir().join(format!("bay-task-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let bundle = dir.join("Demo.eapp");
        std::fs::create_dir_all(&bundle).unwrap();
        let source = "demo.AppImage";
        std::fs::write(bundle.join(source), appimage_bytes(b"old")).unwrap();
        std::fs::write(
            bundle.join("entry.json"),
            r#"{"kind":"appimage","id":"demo","name":"Demo","scope":"user","source":"demo.AppImage","version":"1"}"#,
        )
        .unwrap();
        let bad = dir.join("bad");
        std::fs::write(&bad, b"not an appimage").unwrap();
        let err = replace_appimage(&bundle, source, &bad, "2").unwrap_err();
        assert!(err.contains("left unchanged"));
        assert_eq!(
            std::fs::read(bundle.join(source)).unwrap(),
            appimage_bytes(b"old")
        );
        assert!(
            std::fs::read_to_string(bundle.join("entry.json"))
                .unwrap()
                .contains("\"version\":\"1\"")
                || std::fs::read_to_string(bundle.join("entry.json"))
                    .unwrap()
                    .contains("\"1\"")
        );

        let good = dir.join("good.AppImage");
        std::fs::write(&good, appimage_bytes(b"new")).unwrap();
        replace_appimage(&bundle, source, &good, "2").unwrap();
        assert_eq!(
            std::fs::read(bundle.join(source)).unwrap(),
            appimage_bytes(b"new")
        );
        let entry = std::fs::read_to_string(bundle.join("entry.json")).unwrap();
        assert!(entry.contains("\"version\": \"2\"") || entry.contains("\"version\":\"2\""));
        assert!(!bundle.join(format!("{source}.bak")).exists());

        let placed = place_new_appimage(&dir, "Player", "player", "9", &good).unwrap();
        assert!(placed.join("entry.json").is_file());
        assert!(placed.join("launch.desktop").is_file());
        assert!(placed.join("run").is_file());
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn appimage_bytes(tail: &[u8]) -> Vec<u8> {
        let mut bytes = vec![0u8; 11];
        bytes[0..4].copy_from_slice(b"\x7fELF");
        bytes[8..11].copy_from_slice(b"AI\x02");
        bytes.extend_from_slice(tail);
        bytes
    }

    fn elf_with_upd_info(info: &str) -> Vec<u8> {
        let mut names = vec![0u8];
        names.extend_from_slice(b".shstrtab\0");
        let upd_name = names.len() as u32;
        names.extend_from_slice(b".upd_info\0");
        let mut payload = info.as_bytes().to_vec();
        payload.push(0);
        let shoff = 64u64;
        let shentsize = 64u16;
        let str_off = shoff + 64 * 3;
        let info_off = str_off + names.len() as u64;
        let mut headers = vec![0u8; 64 * 3];
        write_sh64(&mut headers[64..128], 1, str_off, names.len() as u64);
        write_sh64(
            &mut headers[128..192],
            upd_name,
            info_off,
            payload.len() as u64,
        );
        let mut elf = vec![0u8; 64];
        elf[0..4].copy_from_slice(b"\x7fELF");
        elf[4] = 2;
        elf[5] = 1;
        elf[0x28..0x30].copy_from_slice(&shoff.to_le_bytes());
        elf[0x3a..0x3c].copy_from_slice(&shentsize.to_le_bytes());
        elf[0x3c..0x3e].copy_from_slice(&3u16.to_le_bytes());
        elf[0x3e..0x40].copy_from_slice(&1u16.to_le_bytes());
        elf.extend_from_slice(&headers);
        elf.extend_from_slice(&names);
        elf.extend_from_slice(&payload);
        elf
    }

    fn write_sh64(ent: &mut [u8], name: u32, offset: u64, size: u64) {
        ent[0..4].copy_from_slice(&name.to_le_bytes());
        ent[24..32].copy_from_slice(&offset.to_le_bytes());
        ent[32..40].copy_from_slice(&size.to_le_bytes());
    }
}
