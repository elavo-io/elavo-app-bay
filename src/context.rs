//! Shared catalog, HTTP, and the list of what is installed.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;

use gtk::glib;

use crate::catalog::Catalog;
use crate::http::Http;
use crate::installed::Installed;
use crate::model::{AppId, Offer};
use crate::task::{self, BeginError, Job, JobKind, Jobs, Phase};

thread_local! {
    static ON_INSTALLED: RefCell<Option<Rc<dyn Fn()>>> = const { RefCell::new(None) };
}

/// The work behind one queued job. It reports its own result with
/// `clear_job` or `fail_job` before it returns.
type Runner = Box<dyn FnOnce() -> Pin<Box<dyn Future<Output = ()>>>>;

struct Watch {
    id: u64,
    hook: Rc<dyn Fn()>,
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
    jobs: RefCell<Jobs>,
    runners: RefCell<HashMap<String, Runner>>,
    pumping: Cell<bool>,
    watches: RefCell<Vec<Watch>>,
    next_watch: std::cell::Cell<u64>,
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
            jobs: RefCell::default(),
            runners: RefCell::default(),
            pumping: Cell::new(false),
            watches: RefCell::default(),
            next_watch: std::cell::Cell::new(1),
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

    pub fn offer(&self, id: &AppId, version: &str, download: &str, update: &str) -> Offer {
        self.installed.borrow().offer(id, version, download, update)
    }

    /// AppImage id to `(version, download, update provider)`.
    pub fn catalog_listings(&self) -> HashMap<String, (String, String, String)> {
        self.catalog
            .borrow()
            .apps
            .iter()
            .filter(|a| a.source != "flatpakref")
            .map(|a| {
                (
                    a.id.clone(),
                    (a.version.clone(), a.download.clone(), a.update.clone()),
                )
            })
            .collect()
    }

    pub fn job(&self, key: &str) -> Option<Job> {
        self.jobs.borrow().get(key).cloned()
    }

    /// Runs `hook` now and whenever a job changes. The id removes it.
    pub fn watch_jobs(&self, hook: impl Fn() + 'static) -> u64 {
        let hook: Rc<dyn Fn()> = Rc::new(hook);
        hook();
        let id = self.next_watch.get();
        self.next_watch.set(id.wrapping_add(1));
        self.watches.borrow_mut().push(Watch { id, hook });
        id
    }

    pub fn unwatch(&self, id: u64) {
        self.watches.borrow_mut().retain(|watch| watch.id != id);
    }

    /// Puts `run` in the install and update queue under `key`. It starts
    /// at once when nothing else is running, or waits its turn. The queue
    /// belongs to the app session, so moving between pages keeps it.
    pub fn enqueue<F, Fut>(
        self: &Rc<Self>,
        key: &str,
        kind: JobKind,
        run: F,
    ) -> Result<(), BeginError>
    where
        F: FnOnce() -> Fut + 'static,
        Fut: Future<Output = ()> + 'static,
    {
        self.jobs.borrow_mut().enqueue(key, kind)?;
        let runner: Runner = Box::new(move || Box::pin(run()));
        self.runners.borrow_mut().insert(key.to_owned(), runner);
        self.notify_jobs();
        self.pump();
        Ok(())
    }

    /// Place in line counting from 1, for a job that has not started.
    pub fn queue_position(&self, key: &str) -> Option<usize> {
        self.jobs.borrow().position(key)
    }

    /// Takes a job that has not started out of the queue.
    pub fn cancel_job(&self, key: &str) -> bool {
        let cancelled = self.jobs.borrow_mut().cancel(key);
        if cancelled {
            self.runners.borrow_mut().remove(key);
            self.notify_jobs();
        }
        cancelled
    }

    /// Runs queued jobs one after another until the queue is empty.
    fn pump(self: &Rc<Self>) {
        if self.pumping.get() {
            return;
        }
        self.pumping.set(true);
        let ctx = self.clone();
        glib::spawn_future_local(async move {
            loop {
                let Some(key) = ctx.jobs.borrow_mut().start_next() else {
                    break;
                };
                let runner = ctx.runners.borrow_mut().remove(&key);
                ctx.notify_jobs();
                if let Some(runner) = runner {
                    runner().await;
                }
                ctx.jobs.borrow_mut().finish(&key);
                ctx.notify_jobs();
            }
            ctx.pumping.set(false);
        });
    }

    pub fn report(&self, key: &str, phase: Phase, fraction: Option<f64>) {
        self.jobs.borrow_mut().progress(key, phase, fraction);
        self.notify_jobs();
    }

    pub fn fail_job(&self, key: &str, message: impl Into<String>) {
        self.jobs.borrow_mut().fail(key, message);
        self.notify_jobs();
    }

    pub fn follow_job(&self, key: &str, kind: JobKind) {
        self.jobs.borrow_mut().follow(key, kind);
        self.notify_jobs();
    }

    pub fn clear_job(&self, key: &str) {
        if self.jobs.borrow_mut().clear(key) {
            self.notify_jobs();
        }
    }

    fn notify_jobs(&self) {
        let hooks: Vec<_> = self
            .watches
            .borrow()
            .iter()
            .map(|watch| watch.hook.clone())
            .collect();
        for hook in hooks {
            hook();
        }
    }

    /// Flatpak succeeded. Refresh what is installed, then the update list.
    /// A failed update check still drops `id` so a stale offer cannot remain.
    pub async fn settle_flatpak(&self, id: Option<&str>) {
        self.refresh_installed().await;
        if self.refresh_updates().await.is_err() {
            let mut next = self.installed.borrow().clone();
            next.updates = task::settle_updates(&next.updates, id, Err(()));
            next.save();
            *self.installed.borrow_mut() = next;
            notify_installed();
        }
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
