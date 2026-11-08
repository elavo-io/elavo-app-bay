//! The three JSON files the feed editor writes.

use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::model::{Curated, Rail, Story};

pub const REMOTE_BASE: &str = "https://elavo.io/app-bay";

#[derive(Clone, Debug, Default)]
pub struct Catalog {
    /// Directory or URL the relative images resolve against.
    pub base: String,
    pub stories: Vec<Story>,
    pub rails: Vec<Rail>,
    pub apps: Vec<Curated>,
}

#[derive(Deserialize)]
struct FeedFile {
    #[serde(default)]
    stories: Vec<Story>,
    #[serde(default)]
    rails: Vec<Rail>,
}

#[derive(Deserialize)]
struct AppFile {
    #[serde(default)]
    apps: Vec<Curated>,
}

impl Catalog {
    /// A directory named by `ELAVO_APP_BAY_CATALOG`, or an empty catalog when
    /// that variable is a URL or unset. The URL is fetched later.
    pub fn from_env() -> Self {
        let Ok(value) = std::env::var("ELAVO_APP_BAY_CATALOG") else {
            return Self::default();
        };
        if value.starts_with("http://") || value.starts_with("https://") {
            return Self {
                base: value.trim_end_matches('/').to_owned(),
                ..Self::default()
            };
        }
        Self::from_dir(Path::new(&value)).unwrap_or_else(|err| {
            eprintln!("app bay catalog: {err}");
            Self::default()
        })
    }

    pub fn remote_base() -> String {
        match std::env::var("ELAVO_APP_BAY_CATALOG") {
            Ok(value) if value.starts_with("http://") || value.starts_with("https://") => {
                value.trim_end_matches('/').to_owned()
            }
            Ok(_) => String::new(),
            Err(_) => REMOTE_BASE.to_owned(),
        }
    }

    pub fn from_dir(dir: &Path) -> Result<Self, String> {
        let feed = read_json::<FeedFile>(&dir.join("feed.json"))?;
        let mut apps = tagged(
            read_json::<AppFile>(&dir.join("appimages.json"))?,
            "appimage",
        );
        apps.extend(tagged(
            read_json::<AppFile>(&dir.join("flatpakrefs.json"))?,
            "flatpakref",
        ));
        Ok(Self {
            base: dir.to_string_lossy().into_owned(),
            stories: feed.stories,
            rails: feed.rails,
            apps,
        })
    }

    pub fn from_parts(base: &str, feed: &[u8], images: &[u8], refs: &[u8]) -> Result<Self, String> {
        let feed: FeedFile = serde_json::from_slice(feed).map_err(|e| e.to_string())?;
        let mut apps = tagged(
            serde_json::from_slice(images).map_err(|e| e.to_string())?,
            "appimage",
        );
        apps.extend(tagged(
            serde_json::from_slice(refs).map_err(|e| e.to_string())?,
            "flatpakref",
        ));
        Ok(Self {
            base: base.trim_end_matches('/').to_owned(),
            stories: feed.stories,
            rails: feed.rails,
            apps,
        })
    }

    pub fn resolve(&self, path: &str) -> String {
        if path.is_empty()
            || path.starts_with("http://")
            || path.starts_with("https://")
            || path.starts_with("file://")
            || Path::new(path).is_absolute()
        {
            return path.to_owned();
        }
        if self.base.starts_with("http://") || self.base.starts_with("https://") {
            format!("{}/{}", self.base, path)
        } else if self.base.is_empty() {
            path.to_owned()
        } else {
            PathBuf::from(&self.base)
                .join(path)
                .to_string_lossy()
                .into_owned()
        }
    }

    pub fn curated(&self, id: &str) -> Option<&Curated> {
        self.apps.iter().find(|a| a.id == id)
    }
}

fn tagged(mut file: AppFile, source: &str) -> Vec<Curated> {
    for app in &mut file.apps {
        if app.source.is_empty() {
            app.source = source.to_owned();
        }
    }
    file.apps
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    serde_json::from_slice(&bytes).map_err(|e| format!("{}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_three_files() {
        let catalog = Catalog::from_parts(
            "/tmp/bay",
            br#"{"stories":[{"id":"a","title":"Hello","style":"hero","surfaces":["discover"]}],"rails":[{"title":"Picks","surface":"categories","apps":["org.gimp.GIMP"]}]}"#,
            br#"{"apps":[{"id":"demo","name":"Demo","download":"https://example.com/d.AppImage"}]}"#,
            br#"{"apps":[{"id":"com.example.App","name":"Extra","download":"https://example.com/a.flatpakref"}]}"#,
        )
        .unwrap();
        assert_eq!(catalog.stories.len(), 1);
        assert!(catalog.stories[0].is_hero());
        assert_eq!(catalog.apps.len(), 2);
        assert_eq!(catalog.apps[0].source, "appimage");
        assert_eq!(catalog.apps[0].update, "");
        assert_eq!(catalog.apps[1].source, "flatpakref");
        assert!(catalog.resolve("images/a.png").ends_with("images/a.png"));
    }
}
