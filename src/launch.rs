//! What `elavo-app-bay` is asked to open from the command line, and where
//! an app's page comes from.

use crate::installed::Installed;
use crate::model::AppId;

pub const USAGE: &str = "Usage: elavo-app-bay [--app ID]

  --app ID   Open the page of one app. ID is a Flathub ID such as
             org.gimp.GIMP, or appimage:ID for an AppImage in the
             Applications folder. When App Bay is already open, its
             window shows that app.";

/// The app named by `--app ID` or `--app=ID`. The last one wins.
pub fn app_arg(args: &[String]) -> Option<String> {
    let mut found = None;
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        let value = if arg == "--app" {
            args.next().map(String::as_str)
        } else {
            arg.strip_prefix("--app=")
        };
        if let Some(value) = value.map(str::trim).filter(|v| !v.is_empty()) {
            found = Some(value.to_owned());
        }
    }
    found
}

/// Where an app's page is built from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PageSource {
    /// The App Bay catalog's entry.
    Catalog,
    /// Flathub's details, or the installed copy if they do not arrive.
    Flathub,
    /// Only what this computer knows about the installed copy.
    Local,
    /// Neither the catalog nor this computer has it.
    Missing,
}

/// A Flatpak from another remote has no Flathub page, so it is shown from
/// what is installed without asking Flathub. An AppImage that left the
/// catalog, or never was in it, is shown from its Applications entry.
pub fn page_source(id: &AppId, in_catalog: bool, installed: &Installed) -> PageSource {
    match id {
        AppId::Flathub(fid) => match installed.flatpaks.get(fid) {
            Some(app) if !app.origin.is_empty() && app.origin != "flathub" => PageSource::Local,
            _ => PageSource::Flathub,
        },
        AppId::AppImage(_) | AppId::FlatpakRef(_) if in_catalog => PageSource::Catalog,
        AppId::AppImage(iid) if installed.images.contains_key(iid) => PageSource::Local,
        AppId::AppImage(_) | AppId::FlatpakRef(_) => PageSource::Missing,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::installed::{FlatpakApp, ImageApp};

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn reads_the_app_argument() {
        assert_eq!(
            app_arg(&args(&["elavo-app-bay", "--app", "org.gimp.GIMP"])).as_deref(),
            Some("org.gimp.GIMP")
        );
        assert_eq!(
            app_arg(&args(&["elavo-app-bay", "--app=appimage:cursor"])).as_deref(),
            Some("appimage:cursor")
        );
        assert_eq!(app_arg(&args(&["elavo-app-bay"])), None);
        assert_eq!(app_arg(&args(&["elavo-app-bay", "--app"])), None);
        assert_eq!(app_arg(&args(&["elavo-app-bay", "--app", " "])), None);
        assert_eq!(
            app_arg(&args(&["x", "--app", "a", "--app", "b"])).as_deref(),
            Some("b")
        );
    }

    #[test]
    fn ids_route_to_the_right_kind() {
        assert_eq!(
            AppId::parse("org.gimp.GIMP"),
            AppId::Flathub("org.gimp.GIMP".into())
        );
        assert_eq!(
            AppId::parse("appimage:cursor"),
            AppId::AppImage("cursor".into())
        );
    }

    fn installed() -> Installed {
        let mut installed = Installed::default();
        for (id, origin) in [("org.gimp.GIMP", "flathub"), ("com.example.Own", "example")] {
            installed.flatpaks.insert(
                id.into(),
                FlatpakApp {
                    name: id.into(),
                    version: "1".into(),
                    scope: "system".into(),
                    origin: origin.into(),
                },
            );
        }
        installed.images.insert(
            "local-tool".into(),
            ImageApp {
                name: "Local Tool".into(),
                version: "1.2".into(),
                path: "/Applications/Local Tool.eapp".into(),
                source: "LocalTool.AppImage".into(),
                update_info: String::new(),
            },
        );
        installed
    }

    #[test]
    fn chooses_where_a_page_comes_from() {
        let installed = installed();
        let source =
            |raw: &str, in_catalog| page_source(&AppId::parse(raw), in_catalog, &installed);
        assert_eq!(source("org.gimp.GIMP", false), PageSource::Flathub);
        assert_eq!(source("org.not.Installed", false), PageSource::Flathub);
        assert_eq!(source("com.example.Own", false), PageSource::Local);
        assert_eq!(source("appimage:cursor", true), PageSource::Catalog);
        assert_eq!(source("appimage:local-tool", true), PageSource::Catalog);
        assert_eq!(source("appimage:local-tool", false), PageSource::Local);
        assert_eq!(source("appimage:gone", false), PageSource::Missing);
        assert_eq!(source("flatpakref:thing", true), PageSource::Catalog);
        assert_eq!(source("flatpakref:thing", false), PageSource::Missing);
    }
}
