//! What is already on this computer: Flatpaks, and AppImages in the
//! Applications folders.

use std::collections::HashMap;
use std::path::PathBuf;

use crate::model::{AppId, Offer};
use crate::process;

#[derive(Clone, Debug, Default)]
pub struct Installed {
    pub flatpaks: HashMap<String, FlatpakApp>,
    pub images: HashMap<String, ImageApp>,
    /// Flatpak id to the version Flathub has ready.
    pub updates: HashMap<String, String>,
}

#[derive(Clone, Debug)]
pub struct FlatpakApp {
    pub name: String,
    pub version: String,
    pub scope: String,
}

#[derive(Clone, Debug)]
pub struct ImageApp {
    pub name: String,
    pub version: String,
    pub path: PathBuf,
}

impl Installed {
    pub async fn load() -> Self {
        let mut installed = Self::default();
        if let Ok(text) = process::text(&[
            "flatpak",
            "list",
            "--app",
            "--columns=application,name,version,origin,installation",
        ])
        .await
        {
            for line in text.lines() {
                let mut cols = line.split('\t');
                let Some(id) = cols.next() else { continue };
                let name = cols.next().unwrap_or(id);
                let version = cols.next().unwrap_or("");
                let _origin = cols.next();
                let scope = cols.next().unwrap_or("system");
                if id.is_empty() {
                    continue;
                }
                installed.flatpaks.insert(
                    id.to_owned(),
                    FlatpakApp {
                        name: name.to_owned(),
                        version: version.to_owned(),
                        scope: scope.to_owned(),
                    },
                );
            }
        }
        if let Ok(text) = process::text(&[
            "flatpak",
            "remote-ls",
            "--updates",
            "--app",
            "--columns=application,version",
        ])
        .await
        {
            for line in text.lines() {
                let mut cols = line.split('\t');
                let Some(id) = cols.next() else { continue };
                let version = cols.next().unwrap_or("").to_owned();
                if installed.flatpaks.contains_key(id) {
                    installed.updates.insert(id.to_owned(), version);
                }
            }
        }
        scan_images(&applications_dirs(), &mut installed.images);
        installed
    }

    pub fn offer(&self, id: &AppId, catalog_version: &str) -> Offer {
        match id {
            AppId::Flathub(fid) | AppId::FlatpakRef(fid) => {
                if self.updates.contains_key(fid) {
                    Offer::Update
                } else if self.flatpaks.contains_key(fid) {
                    Offer::Open
                } else {
                    Offer::Get
                }
            }
            AppId::AppImage(iid) => match self.images.get(iid) {
                Some(app)
                    if !catalog_version.is_empty()
                        && app.version != catalog_version
                        && !app.version.is_empty() =>
                {
                    Offer::Update
                }
                Some(_) => Offer::Open,
                None => Offer::Get,
            },
        }
    }

    pub fn update_count(&self, catalog_versions: &HashMap<String, String>) -> u32 {
        let mut n = self.updates.len() as u32;
        for (id, app) in &self.images {
            if let Some(version) = catalog_versions.get(id) {
                if !version.is_empty() && !app.version.is_empty() && app.version != *version {
                    n += 1;
                }
            }
        }
        n
    }
}

fn applications_dirs() -> Vec<PathBuf> {
    vec![
        gtk::glib::home_dir().join("Applications"),
        PathBuf::from("/Applications"),
    ]
}

fn scan_images(dirs: &[PathBuf], into: &mut HashMap<String, ImageApp>) {
    for dir in dirs {
        let Ok(read) = std::fs::read_dir(dir) else {
            continue;
        };
        for entry in read.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("eapp") {
                continue;
            }
            let Ok(bytes) = std::fs::read(path.join("entry.json")) else {
                continue;
            };
            let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
                continue;
            };
            if value.get("kind").and_then(|v| v.as_str()) != Some("appimage") {
                continue;
            }
            let Some(id) = value.get("id").and_then(|v| v.as_str()) else {
                continue;
            };
            let name = value.get("name").and_then(|v| v.as_str()).unwrap_or(id);
            let version = value.get("version").and_then(|v| v.as_str()).unwrap_or("");
            into.insert(
                id.to_owned(),
                ImageApp {
                    name: name.to_owned(),
                    version: version.to_owned(),
                    path,
                },
            );
        }
    }
}
