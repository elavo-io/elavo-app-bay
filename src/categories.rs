//! The category grid. Ids are the closed set the feed editor checks.
//! Flathub's own category slugs are the lowercase names its collection
//! route uses. A hub alias opens that hub instead of a second page.

use crate::i18n::tr;

pub struct Category {
    pub id: &'static str,
    pub icon: &'static str,
    /// Flathub collection slug, when the category has one.
    pub flathub: Option<&'static str>,
    /// Sidebar page this row opens, when it is the same page as a hub.
    pub hub: Option<&'static str>,
}

pub fn all() -> &'static [Category] {
    &CATEGORIES
}

pub fn title(id: &str) -> String {
    match id {
        "business" => tr("Business"),
        "development" => tr("Developer Tools"),
        "education" => tr("Education"),
        "entertainment" => tr("Entertainment"),
        "finance" => tr("Finance"),
        "games" => tr("Games"),
        "graphics" => tr("Graphics & Design"),
        "health" => tr("Health & Fitness"),
        "lifestyle" => tr("Lifestyle"),
        "medical" => tr("Medical"),
        "music" => tr("Music"),
        "news" => tr("News"),
        "photo-video" => tr("Photo & Video"),
        "productivity" => tr("Productivity"),
        "reference" => tr("Reference"),
        "social" => tr("Social Networking"),
        "sports" => tr("Sports"),
        "travel" => tr("Travel"),
        "utility" => tr("Utilities"),
        "weather" => tr("Weather"),
        _ => id.to_owned(),
    }
}

pub fn get(id: &str) -> Option<&'static Category> {
    CATEGORIES.iter().find(|c| c.id == id)
}

/// Flathub slugs a hub's ranking is drawn from, in the order they are merged.
pub fn hub_slugs(hub: &str) -> &'static [&'static str] {
    match hub {
        "create" => &["graphics", "audiovideo"],
        "work" => &["office"],
        "play" => &["game"],
        "develop" => &["development"],
        _ => &[],
    }
}

const CATEGORIES: [Category; 20] = [
    Category {
        id: "business",
        icon: "x-office-document-symbolic",
        flathub: Some("office"),
        hub: None,
    },
    Category {
        id: "development",
        icon: "applications-engineering-symbolic",
        flathub: Some("development"),
        hub: Some("develop"),
    },
    Category {
        id: "education",
        icon: "accessories-dictionary-symbolic",
        flathub: Some("education"),
        hub: None,
    },
    Category {
        id: "entertainment",
        icon: "applications-multimedia-symbolic",
        flathub: Some("audiovideo"),
        hub: None,
    },
    Category {
        id: "finance",
        icon: "x-office-spreadsheet-symbolic",
        flathub: None,
        hub: None,
    },
    Category {
        id: "games",
        icon: "applications-games-symbolic",
        flathub: Some("game"),
        hub: Some("play"),
    },
    Category {
        id: "graphics",
        icon: "applications-graphics-symbolic",
        flathub: Some("graphics"),
        hub: None,
    },
    Category {
        id: "health",
        icon: "applications-science-symbolic",
        flathub: Some("healthfitness"),
        hub: None,
    },
    Category {
        id: "lifestyle",
        icon: "emblem-favorite-symbolic",
        flathub: None,
        hub: None,
    },
    Category {
        id: "medical",
        icon: "applications-science-symbolic",
        flathub: None,
        hub: None,
    },
    Category {
        id: "music",
        icon: "audio-x-generic-symbolic",
        flathub: Some("audiovideo"),
        hub: None,
    },
    Category {
        id: "news",
        icon: "newspaper-symbolic",
        flathub: Some("network"),
        hub: None,
    },
    Category {
        id: "photo-video",
        icon: "camera-photo-symbolic",
        flathub: Some("graphics"),
        hub: None,
    },
    Category {
        id: "productivity",
        icon: "x-office-document-symbolic",
        flathub: Some("office"),
        hub: None,
    },
    Category {
        id: "reference",
        icon: "accessories-dictionary-symbolic",
        flathub: None,
        hub: None,
    },
    Category {
        id: "social",
        icon: "mail-unread-symbolic",
        flathub: Some("network"),
        hub: None,
    },
    Category {
        id: "sports",
        icon: "applications-games-symbolic",
        flathub: None,
        hub: None,
    },
    Category {
        id: "travel",
        icon: "mark-location-symbolic",
        flathub: None,
        hub: None,
    },
    Category {
        id: "utility",
        icon: "applications-utilities-symbolic",
        flathub: Some("utility"),
        hub: None,
    },
    Category {
        id: "weather",
        icon: "weather-clear-symbolic",
        flathub: None,
        hub: None,
    },
];
