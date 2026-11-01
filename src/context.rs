//! Shared catalog, HTTP, and the list of what is installed.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::catalog::Catalog;
use crate::http::Http;
use crate::installed::Installed;
use crate::model::{AppId, Offer};

thread_local! {
    static ON_INSTALLED: RefCell<Option<Rc<dyn Fn()>>> = const { RefCell::new(None) };
}

/// The window registers this so the Updates badge follows a finished check.
pub fn on_installed_change(hook: impl Fn() + 'static) {
    ON_INSTALLED.with(|slot| *slot.borrow_mut() = Some(Rc::new(hook)));
}

fn notify_installed() {
    ON_INSTALLED.with(|slot| {
        if let Some(hook) = slot.borrow().clone() {
            hook();
        }
    });
}

pub struct Ctx {
    pub catalog: RefCell<Catalog>,
    pub http: Http,
    pub installed: RefCell<Installed>,
}

impl Ctx {
    pub fn new() -> Rc<Self> {
        let http = Http::new();
        let catalog = if Catalog::remote_base().is_empty() {
            Catalog::from_env()
        } else {
            stored_remote(&http).unwrap_or_else(Catalog::from_env)
        };
        Rc::new(Self {
            catalog: RefCell::new(catalog),
            http,
            installed: RefCell::new(Installed::read_index()),
        })
    }

    /// Fetch the Elavo catalog on every launch. The saved copy is shown
    /// immediately and kept when elavo.io does not answer.
    pub async fn refresh_remote(&self) {
        let base = Catalog::remote_base();
        if base.is_empty() {
            return;
        }
        let feed = self.http.get_latest(&format!("{base}/feed.json"));
        let images = self.http.get_latest(&format!("{base}/appimages.json"));
        let refs = self.http.get_latest(&format!("{base}/flatpakrefs.json"));
        let (feed, images, refs) = (feed.await, images.await, refs.await);
        let (Ok(feed), Ok(images), Ok(refs)) = (feed, images, refs) else {
            return;
        };
        if let Ok(catalog) = Catalog::from_parts(&base, &feed, &images, &refs) {
            *self.catalog.borrow_mut() = catalog;
        }
    }

    /// Reread what is installed on this computer. Does not ask remotes
    /// about updates; that wait belongs on the Updates page.
    pub async fn refresh_installed(&self) {
        let mut next = self.installed.borrow().clone();
        let before = next.revision();
        next.reload_local().await;
        if next.revision() == before {
            return;
        }
        next.save();
        *self.installed.borrow_mut() = next;
        notify_installed();
    }

    /// Ask the Flatpak remotes which installed apps have a newer build.
    pub async fn refresh_updates(&self) -> Result<(), String> {
        let mut next = self.installed.borrow().clone();
        next.reload_local().await;
        next.reload_updates().await?;
        next.save();
        *self.installed.borrow_mut() = next;
        notify_installed();
        Ok(())
    }

    pub fn offer(&self, id: &AppId, version: &str) -> Offer {
        self.installed.borrow().offer(id, version)
    }

    pub fn catalog_versions(&self) -> HashMap<String, String> {
        self.catalog
            .borrow()
            .apps
            .iter()
            .filter(|a| a.source != "flatpakref")
            .map(|a| (a.id.clone(), a.version.clone()))
            .collect()
    }
}

fn stored_remote(http: &Http) -> Option<Catalog> {
    let base = Catalog::remote_base();
    if base.is_empty() {
        return None;
    }
    let feed = http.stored("GET", &format!("{base}/feed.json"), None)?;
    let images = http.stored("GET", &format!("{base}/appimages.json"), None)?;
    let refs = http.stored("GET", &format!("{base}/flatpakrefs.json"), None)?;
    Catalog::from_parts(&base, &feed, &images, &refs).ok()
}
