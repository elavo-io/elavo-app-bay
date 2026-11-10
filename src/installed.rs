//! What is already on this computer: Flatpaks, and AppImages in the
//! Applications folders.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::http;
use crate::model::{AppId, Offer};
use crate::process;
use crate::task::{self, ImagePlan};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Installed {
    pub flatpaks: HashMap<String, FlatpakApp>,
    pub images: HashMap<String, ImageApp>,
    /// Flatpak id to the version Flathub has ready.
    pub updates: HashMap<String, String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FlatpakApp {
    pub name: String,
    pub version: String,
    pub scope: String,
    /// The remote the app was installed from, such as `flathub`.
    #[serde(default)]
    pub origin: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ImageApp {
    pub name: String,
    pub version: String,
    pub path: PathBuf,
    /// File name of the AppImage inside `path`.
    #[serde(default)]
    pub source: String,
    /// Embedded AppImage update information, when the file has any.
    #[serde(default)]
    pub update_info: String,
}

impl Installed {
    /// The last list this computer saved. Opening Installed does not wait
    /// on a Flatpak remote to rebuild it.
    pub fn read_index() -> Self {
        let Ok(bytes) = std::fs::read(index_path()) else {
            return Self::default();
        };
        serde_json::from_slice(&bytes).unwrap_or_default()
    }

    pub fn save(&self) {
        let Ok(bytes) = serde_json::to_vec_pretty(self) else {
            return;
        };
        let path = index_path();
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(path, bytes);
    }

    /// Apps installed here. This does not ask remotes which of them updated.
    pub async fn reload_local(&mut self) {
        if let Ok(text) = process::text(&[
            "flatpak",
            "list",
            "--app",
            "--columns=application,name,version,origin,installation",
        ])
        .await
        {
            self.flatpaks = parse_flatpaks(&text);
            self.updates.retain(|id, _| self.flatpaks.contains_key(id));
        }
        let mut images = HashMap::new();
        scan_images(&applications_dirs(), &mut images);
        self.images = images;
    }

    /// Which installed Flatpaks have a newer build. This talks to the remotes.
    pub async fn reload_updates(&mut self) -> Result<(), String> {
        let text = process::text(&[
            "flatpak",
            "remote-ls",
            "--updates",
            "--app",
            "--columns=application,version",
        ])
        .await?;
        let mut updates = HashMap::new();
        for line in text.lines() {
            let mut cols = line.split('\t');
            let Some(id) = cols.next() else { continue };
            let version = cols.next().unwrap_or("").to_owned();
            if self.flatpaks.contains_key(id) {
                updates.insert(id.to_owned(), version);
            }
        }
        self.updates = updates;
        Ok(())
    }

    pub fn revision(&self) -> String {
        let mut flatpaks: Vec<_> = self
            .flatpaks
            .iter()
            .map(|(id, app)| format!("{id}\t{}\t{}\t{}", app.name, app.version, app.scope))
            .collect();
        flatpaks.sort();
        let mut images: Vec<_> = self
            .images
            .iter()
            .map(|(id, app)| format!("{id}\t{}\t{}", app.name, app.version))
            .collect();
        images.sort();
        let mut updates: Vec<_> = self
            .updates
            .iter()
            .map(|(id, version)| format!("{id}\t{version}"))
            .collect();
        updates.sort();
        format!(
            "{}\n{}\n{}",
            flatpaks.join("\n"),
            images.join("\n"),
            updates.join("\n")
        )
    }

    pub fn offer(&self, id: &AppId, catalog_version: &str, download: &str, update: &str) -> Offer {
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
                Some(app) => match task::image_plan(
                    &app.version,
                    catalog_version,
                    download,
                    update,
                    &app.update_info,
                ) {
                    ImagePlan::Ready(_) => Offer::Update,
                    _ => Offer::Open,
                },
                None => Offer::Get,
            },
        }
    }

    pub fn image_plan_for(
        &self,
        id: &str,
        catalog_version: &str,
        download: &str,
        update: &str,
    ) -> ImagePlan {
        let Some(app) = self.images.get(id) else {
            return ImagePlan::NotInstalled;
        };
        task::image_plan(
            &app.version,
            catalog_version,
            download,
            update,
            &app.update_info,
        )
    }

    pub fn update_count(&self, listings: &HashMap<String, (String, String, String)>) -> u32 {
        let mut n = self.updates.len() as u32;
        for (id, app) in &self.images {
            let Some((version, download, update)) = listings.get(id) else {
                continue;
            };
            if matches!(
                task::image_plan(&app.version, version, download, update, &app.update_info),
                ImagePlan::Ready(_)
            ) {
                n += 1;
            }
        }
        n
    }
}

fn index_path() -> PathBuf {
    http::cache_dir().join("installed.json")
}

fn parse_flatpaks(text: &str) -> HashMap<String, FlatpakApp> {
    let mut flatpaks = HashMap::new();
    for line in text.lines() {
        let mut cols = line.split('\t');
        let Some(id) = cols.next() else { continue };
        let name = cols.next().unwrap_or(id);
        let version = cols.next().unwrap_or("");
        let origin = cols.next().unwrap_or("");
        let scope = cols.next().unwrap_or("system");
        if id.is_empty() {
            continue;
        }
        flatpaks.insert(
            id.to_owned(),
            FlatpakApp {
                name: name.to_owned(),
                version: version.to_owned(),
                scope: scope.to_owned(),
                origin: origin.to_owned(),
            },
        );
    }
    flatpaks
}

/// The icon an `.eapp` bundle carries, if any.
pub fn bundle_icon(bundle: &Path) -> Option<PathBuf> {
    ["icon.png", "icon.svg", "standard-icon.png"]
        .iter()
        .map(|name| bundle.join(name))
        .find(|path| path.is_file())
}

/// The icon an installed Flatpak exports, the user installation first.
pub fn flatpak_icon(id: &str) -> Option<PathBuf> {
    let bases = [
        gtk::glib::user_data_dir().join("flatpak/exports/share/icons/hicolor"),
        PathBuf::from("/var/lib/flatpak/exports/share/icons/hicolor"),
    ];
    bases.iter().find_map(|base| {
        flatpak_icon_names(id)
            .into_iter()
            .map(|name| base.join(name))
            .find(|path| path.is_file())
    })
}

fn flatpak_icon_names(id: &str) -> Vec<String> {
    let mut names = vec![format!("scalable/apps/{id}.svg")];
    names.extend(
        ["512x512", "256x256", "128x128", "64x64"].map(|size| format!("{size}/apps/{id}.png")),
    );
    names
}

fn applications_dirs() -> Vec<PathBuf> {
    vec![
        gtk::glib::home_dir().join("Applications"),
        PathBuf::from("/Applications"),
    ]
}

fn appimage_in(dir: &std::path::Path) -> Option<PathBuf> {
    let read = std::fs::read_dir(dir).ok()?;
    read.flatten().map(|entry| entry.path()).find(|path| {
        path.extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("appimage"))
    })
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
            let source = value.get("source").and_then(|v| v.as_str()).unwrap_or("");
            let image = if source.is_empty() {
                appimage_in(&path)
            } else {
                Some(path.join(source))
            };
            let update_info = image
                .as_ref()
                .and_then(|file| task::read_update_info(file))
                .unwrap_or_default();
            into.insert(
                id.to_owned(),
                ImageApp {
                    name: name.to_owned(),
                    version: version.to_owned(),
                    path,
                    source: source.to_owned(),
                    update_info,
                },
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_index_round_trips() {
        let mut installed = Installed::default();
        installed.flatpaks.insert(
            "org.gimp.GIMP".into(),
            FlatpakApp {
                name: "GIMP".into(),
                version: "3.0".into(),
                scope: "system".into(),
                origin: "flathub".into(),
            },
        );
        installed
            .updates
            .insert("org.gimp.GIMP".into(), "3.0.2".into());
        let bytes = serde_json::to_vec(&installed).unwrap();
        let back: Installed = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(back.flatpaks["org.gimp.GIMP"].version, "3.0");
        assert_eq!(back.updates["org.gimp.GIMP"], "3.0.2");
        assert_eq!(installed.revision(), back.revision());
    }

    #[test]
    fn a_bundle_icon_is_found_in_order() {
        let dir = std::env::temp_dir().join(format!("app-bay-icon-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(bundle_icon(&dir), None);
        std::fs::write(dir.join("standard-icon.png"), b"png").unwrap();
        assert_eq!(bundle_icon(&dir), Some(dir.join("standard-icon.png")));
        std::fs::write(dir.join("icon.svg"), b"svg").unwrap();
        assert_eq!(bundle_icon(&dir), Some(dir.join("icon.svg")));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn flatpak_icons_prefer_scalable_then_large() {
        let names = flatpak_icon_names("org.gimp.GIMP");
        assert_eq!(names[0], "scalable/apps/org.gimp.GIMP.svg");
        assert_eq!(names[1], "512x512/apps/org.gimp.GIMP.png");
    }

    #[test]
    fn flatpak_list_rows_keep_the_installation() {
        let parsed = parse_flatpaks("org.gimp.GIMP\tGIMP\t3.0\tflathub\tuser\n");
        assert_eq!(parsed["org.gimp.GIMP"].scope, "user");
        assert_eq!(parsed["org.gimp.GIMP"].origin, "flathub");
        assert_eq!(parsed["org.gimp.GIMP"].name, "GIMP");
    }

    #[test]
    fn an_appimage_without_update_information_is_not_an_update() {
        let mut installed = Installed::default();
        installed.images.insert(
            "demo".into(),
            ImageApp {
                name: "Demo".into(),
                version: "1".into(),
                path: PathBuf::from("/tmp/Demo.eapp"),
                source: "demo.AppImage".into(),
                update_info: String::new(),
            },
        );
        assert_eq!(
            installed.offer(&AppId::AppImage("demo".into()), "2", "", ""),
            Offer::Open
        );
        assert_eq!(
            installed.offer(
                &AppId::AppImage("demo".into()),
                "2",
                "https://example.com/demo.AppImage",
                ""
            ),
            Offer::Update
        );
        let mut listings = HashMap::new();
        listings.insert(
            "demo".into(),
            (
                "2".into(),
                "https://example.com/demo.AppImage".into(),
                String::new(),
            ),
        );
        assert_eq!(installed.update_count(&listings), 1);
    }
}
