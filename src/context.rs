//! Shared catalog, HTTP, and the list of what is installed.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::catalog::Catalog;
use crate::http::Http;
use crate::installed::Installed;
use crate::model::{AppId, Offer};

pub struct Ctx {
    pub catalog: RefCell<Catalog>,
    pub http: Http,
    pub installed: RefCell<Installed>,
}

impl Ctx {
    pub fn new() -> Rc<Self> {
        Rc::new(Self {
            catalog: RefCell::new(Catalog::from_env()),
            http: Http::new(),
            installed: RefCell::new(Installed::default()),
        })
    }

    pub async fn refresh_remote(&self) {
        let base = Catalog::remote_base();
        if base.is_empty() {
            return;
        }
        let feed = self.http.get(&format!("{base}/feed.json"), 60 * 60);
        let images = self.http.get(&format!("{base}/appimages.json"), 60 * 60);
        let refs = self.http.get(&format!("{base}/flatpakrefs.json"), 60 * 60);
        let (feed, images, refs) = (feed.await, images.await, refs.await);
        let (Ok(feed), Ok(images), Ok(refs)) = (feed, images, refs) else {
            return;
        };
        if let Ok(catalog) = Catalog::from_parts(&base, &feed, &images, &refs) {
            *self.catalog.borrow_mut() = catalog;
        }
    }

    pub async fn refresh_installed(&self) {
        *self.installed.borrow_mut() = Installed::load().await;
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
