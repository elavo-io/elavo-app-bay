//! Catalog records and the ids that tie a row to Flathub, an AppImage, or a
//! flatpakref.

use serde::Deserialize;

/// Where an app row installs from.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum AppId {
    Flathub(String),
    AppImage(String),
    FlatpakRef(String),
}

impl AppId {
    pub fn parse(raw: &str) -> Self {
        if let Some(id) = raw.strip_prefix("appimage:") {
            Self::AppImage(id.to_owned())
        } else if let Some(id) = raw.strip_prefix("flatpakref:") {
            Self::FlatpakRef(id.to_owned())
        } else if let Some(id) = raw.strip_prefix("flathub:") {
            Self::Flathub(id.to_owned())
        } else {
            Self::Flathub(raw.to_owned())
        }
    }

    pub fn key(&self) -> String {
        match self {
            Self::Flathub(id) => format!("flathub:{id}"),
            Self::AppImage(id) => format!("appimage:{id}"),
            Self::FlatpakRef(id) => format!("flatpakref:{id}"),
        }
    }
}

/// What the pill button offers for an app that is, or is not, on this computer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Offer {
    Get,
    Open,
    Update,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Story {
    pub id: String,
    #[serde(default)]
    pub kicker: String,
    pub title: String,
    #[serde(default)]
    pub subtitle: String,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub image: String,
    #[serde(default)]
    pub color: String,
    #[serde(default)]
    pub date: String,
    /// `hero` or `card`.
    #[serde(default = "card_style")]
    pub style: String,
    #[serde(default)]
    pub surfaces: Vec<String>,
    #[serde(default)]
    pub apps: Vec<String>,
}

fn card_style() -> String {
    "card".into()
}

impl Story {
    pub fn is_hero(&self) -> bool {
        self.style == "hero"
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct Rail {
    /// Kept so a saved catalog round-trips through the editor.
    #[serde(default)]
    #[allow(dead_code)]
    pub id: String,
    pub title: String,
    pub surface: String,
    #[serde(default)]
    pub apps: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Curated {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub developer: String,
    #[serde(default)]
    pub categories: Vec<String>,
    #[serde(default)]
    pub icon: String,
    /// Download URL for an AppImage, or the flatpakref URL.
    #[serde(default)]
    pub download: String,
    /// Update provider for an AppImage: `zsync|https://…`, a GitHub
    /// `gh-releases-zsync|…` string, or a `.zsync` URL. Empty when the
    /// listing has none. A newer `version` by itself is not an update.
    #[serde(default)]
    pub update: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub homepage: String,
    #[serde(default)]
    pub source: String,
}

impl Curated {
    pub fn app_id(&self) -> AppId {
        if self.source == "flatpakref" {
            AppId::FlatpakRef(self.id.clone())
        } else {
            AppId::AppImage(self.id.clone())
        }
    }
}

/// A row's display fields, from Flathub or from the catalog.
#[derive(Clone, Debug)]
pub struct Brief {
    pub id: AppId,
    pub name: String,
    pub summary: String,
    pub icon: String,
    pub developer: String,
    pub version: String,
    pub download: String,
    /// Catalog update provider. Empty for Flathub rows.
    pub update: String,
}

/// One editorial surface: Discover, a hub, or a category id.
pub fn stories_for<'a>(stories: &'a [Story], surface: &str) -> Vec<&'a Story> {
    let mut hero = Vec::new();
    let mut cards = Vec::new();
    for story in stories {
        if story.surfaces.iter().any(|s| s == surface) {
            if story.is_hero() {
                hero.push(story);
            } else {
                cards.push(story);
            }
        }
    }
    hero.extend(cards);
    hero
}

pub fn rails_for<'a>(rails: &'a [Rail], surface: &str) -> Vec<&'a Rail> {
    rails.iter().filter(|r| r.surface == surface).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_prefixed_ids() {
        assert_eq!(
            AppId::parse("flathub:org.gimp.GIMP"),
            AppId::Flathub("org.gimp.GIMP".into())
        );
        assert_eq!(
            AppId::parse("org.gimp.GIMP"),
            AppId::Flathub("org.gimp.GIMP".into())
        );
        assert_eq!(
            AppId::parse("appimage:kdenlive"),
            AppId::AppImage("kdenlive".into())
        );
        assert_eq!(
            AppId::parse("flatpakref:com.example.App").key(),
            "flatpakref:com.example.App"
        );
    }
}
