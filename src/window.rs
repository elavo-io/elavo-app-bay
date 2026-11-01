//! The sidebar window. Back and Forward sit in the content band. The large
//! page title scrolls with the page and moves into the band once it has
//! gone past.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;

use gtk::glib;
use gtk::prelude::*;

use crate::context::Ctx;
use crate::i18n::tr;
use crate::nav::{Nav, Route, Sidebar};
use crate::pages;

const SIDEBAR_WIDTH: i32 = 220;

type Pending = Rc<RefCell<Option<Box<dyn FnOnce()>>>>;

struct SidebarItem {
    id: &'static str,
    icon: &'static str,
    title: fn() -> String,
    badge: bool,
}

pub struct Window {
    pub window: gtk::ApplicationWindow,
    ctx: Rc<Ctx>,
    stack: gtk::Stack,
    sidebar: gtk::ListBox,
    rows: Vec<gtk::ListBoxRow>,
    ids: Vec<&'static str>,
    title: gtk::Label,
    back: gtk::Button,
    forward: gtk::Button,
    search: gtk::SearchEntry,
    badge: gtk::Label,
    current: RefCell<Route>,
    history: RefCell<Vec<Route>>,
    future: RefCell<Vec<Route>>,
    guard: Cell<bool>,
    scroll: RefCell<Option<(gtk::Adjustment, glib::SignalHandlerId)>>,
    confirm_window: gtk::Window,
    confirm_label: gtk::Label,
    confirm_yes: Pending,
    nav: RefCell<Option<Nav>>,
    titles: RefCell<HashMap<String, String>>,
}

impl Window {
    pub fn new(app: &gtk::Application, ctx: Rc<Ctx>) -> Rc<Self> {
        let window = gtk::ApplicationWindow::builder()
            .application(app)
            .title(tr("App Bay"))
            .icon_name(crate::APP_ID)
            .default_width(1100)
            .default_height(740)
            .show_menubar(false)
            .build();

        let back = nav_button("go-previous-symbolic", &tr("Back"));
        let forward = nav_button("go-next-symbolic", &tr("Forward"));
        let nav_box = gtk::Box::builder().css_classes(["settings-nav"]).build();
        let rule = gtk::Box::builder()
            .width_request(1)
            .css_classes(["settings-nav-rule"])
            .build();
        nav_box.append(&back);
        nav_box.append(&rule);
        nav_box.append(&forward);
        let title = gtk::Label::builder()
            .css_classes(["pane-title"])
            .xalign(0.0)
            .hexpand(true)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .visible(false)
            .build();
        let content_header = gtk::Box::builder()
            .spacing(12)
            .hexpand(true)
            .vexpand(true)
            .css_classes(["settings-content-header"])
            .build();
        nav_box.set_valign(gtk::Align::Center);
        title.set_valign(gtk::Align::Center);
        content_header.append(&nav_box);
        content_header.append(&title);

        let sidebar_bar = gtk::HeaderBar::builder()
            .show_title_buttons(true)
            .css_classes(["settings-sidebar-bar"])
            .build();
        sidebar_bar.set_title_widget(Some(&gtk::Box::new(gtk::Orientation::Horizontal, 0)));
        let chrome = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        chrome.append(&sidebar_bar);
        chrome.append(&gtk::Separator::new(gtk::Orientation::Vertical));
        chrome.append(&content_header);
        window.set_titlebar(Some(&gtk::WindowHandle::builder().child(&chrome).build()));

        let search = gtk::SearchEntry::builder()
            .placeholder_text(tr("Search"))
            .build();
        let sidebar = gtk::ListBox::builder()
            .selection_mode(gtk::SelectionMode::Single)
            .css_classes(["navigation-sidebar"])
            .build();
        let items = sidebar_items();
        let badge = gtk::Label::builder()
            .visible(false)
            .css_classes(["count-badge"])
            .build();
        let mut rows = Vec::new();
        let mut ids = Vec::new();
        for item in &items {
            let row = sidebar_row(item, &badge);
            sidebar.append(&row);
            ids.push(item.id);
            rows.push(row);
        }

        // Set, so a child that wants to grow cannot widen this column past
        // the window-button band.
        let side = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .width_request(SIDEBAR_WIDTH)
            .hexpand(false)
            .css_classes(["settings-sidebar", "bay-side"])
            .build();
        sidebar_bar.set_width_request(SIDEBAR_WIDTH);
        let columns = gtk::SizeGroup::new(gtk::SizeGroupMode::Horizontal);
        columns.add_widget(&sidebar_bar);
        columns.add_widget(&side);
        side.append(&search);
        side.append(
            &gtk::ScrolledWindow::builder()
                .hscrollbar_policy(gtk::PolicyType::Never)
                .vexpand(true)
                .child(&sidebar)
                .build(),
        );

        let stack = gtk::Stack::builder().hexpand(true).vexpand(true).build();
        let body = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        body.append(&side);
        body.append(&gtk::Separator::new(gtk::Orientation::Vertical));
        body.append(&stack);
        window.set_child(Some(&body));

        let (confirm_window, confirm_label, confirm_yes) = confirm_dialog(&window);

        let this = Rc::new(Self {
            window,
            ctx,
            stack,
            sidebar,
            rows,
            ids,
            title,
            back,
            forward,
            search,
            badge,
            current: RefCell::new(Route::Discover),
            history: RefCell::default(),
            future: RefCell::default(),
            guard: Cell::new(false),
            scroll: RefCell::new(None),
            confirm_window,
            confirm_label,
            confirm_yes,
            nav: RefCell::new(None),
            titles: RefCell::new(HashMap::new()),
        });
        this.bind();
        this.show(Route::Discover, false);
        let weak = Rc::downgrade(&this);
        crate::context::on_installed_change(move || {
            if let Some(window) = weak.upgrade() {
                window.refresh_badge();
            }
        });
        this
    }

    pub fn nav(&self) -> Nav {
        self.nav.borrow().clone().expect("navigation")
    }

    pub fn refresh_badge(&self) {
        let n = self
            .ctx
            .installed
            .borrow()
            .update_count(&self.ctx.catalog_versions());
        self.badge.set_label(&n.to_string());
        self.badge.set_visible(n > 0);
    }

    /// Build the current page again. Used after the catalog or the install
    /// list arrives.
    pub fn reload(self: &Rc<Self>) {
        let route = self.current.borrow().clone();
        self.drop_page(&route);
        self.present(&route);
    }

    fn bind(self: &Rc<Self>) {
        let weak = Rc::downgrade(self);
        *self.nav.borrow_mut() = Some(Nav::new(
            {
                let weak = weak.clone();
                move |route| {
                    if let Some(w) = weak.upgrade() {
                        w.show(route, true);
                    }
                }
            },
            {
                let weak = weak.clone();
                move |text, yes| {
                    if let Some(w) = weak.upgrade() {
                        w.ask(text, yes);
                    }
                }
            },
            {
                let weak = weak.clone();
                move |title| {
                    if let Some(w) = weak.upgrade() {
                        let key = w.current.borrow().key();
                        w.titles.borrow_mut().insert(key, title.clone());
                        w.title.set_label(&title);
                    }
                }
            },
        ));

        let weak = Rc::downgrade(self);
        self.sidebar.connect_row_selected(move |_, row| {
            let Some(w) = weak.upgrade() else { return };
            if w.guard.get() {
                return;
            }
            let Some(row) = row else { return };
            let Some(id) = w.ids.get(row.index() as usize).copied() else {
                return;
            };
            w.clear_search();
            w.show(Route::from_sidebar(id), true);
        });

        let weak = Rc::downgrade(self);
        self.back.connect_clicked(move |_| {
            if let Some(w) = weak.upgrade() {
                w.go_back();
            }
        });
        let weak = Rc::downgrade(self);
        self.forward.connect_clicked(move |_| {
            if let Some(w) = weak.upgrade() {
                w.go_forward();
            }
        });

        let generation = Rc::new(Cell::new(0u32));
        let weak = Rc::downgrade(self);
        let generation_changed = generation.clone();
        self.search.connect_search_changed(move |entry| {
            let Some(w) = weak.upgrade() else { return };
            if w.guard.get() {
                return;
            }
            let n = generation_changed.get().wrapping_add(1);
            generation_changed.set(n);
            let text = entry.text().trim().to_owned();
            let weak = weak.clone();
            let generation = generation_changed.clone();
            glib::timeout_add_local(Duration::from_millis(250), move || {
                if generation.get() != n {
                    return glib::ControlFlow::Break;
                }
                if let Some(w) = weak.upgrade() {
                    w.submit_search(text.clone());
                }
                glib::ControlFlow::Break
            });
        });
        let weak = Rc::downgrade(self);
        self.search.connect_activate(move |entry| {
            if let Some(w) = weak.upgrade() {
                generation.set(generation.get().wrapping_add(1));
                w.submit_search(entry.text().trim().to_owned());
            }
        });

        let weak = Rc::downgrade(self);
        self.confirm_window.connect_close_request(move |_| {
            if let Some(w) = weak.upgrade() {
                w.confirm_yes.borrow_mut().take();
                w.confirm_window.set_visible(false);
            }
            glib::Propagation::Stop
        });
    }

    fn show(self: &Rc<Self>, route: Route, record: bool) {
        let current = self.current.borrow().clone();
        let built = self.stack.child_by_name(&route.key()).is_some();
        if current == route && built {
            if matches!(route, Route::Updates | Route::Installed) {
                self.drop_page(&route);
            } else {
                return;
            }
        }
        if record && current != route {
            self.history.borrow_mut().push(current.clone());
            if self.history.borrow().len() > 50 {
                self.history.borrow_mut().remove(0);
            }
            self.future.borrow_mut().clear();
        }
        if matches!(route, Route::Search(_)) {
            if let Route::Search(ref previous) = current {
                self.drop_page(&Route::Search(previous.clone()));
            }
        }
        self.current.replace(route.clone());
        self.present(&route);
        self.sync_buttons();
    }

    fn present(&self, route: &Route) {
        let key = route.key();
        if self.stack.child_by_name(&key).is_none() {
            let nav = self.nav.borrow().clone().expect("navigation");
            let page = pages::build(route, &self.ctx, &nav);
            self.titles.borrow_mut().insert(key.clone(), page.title);
            self.stack.add_named(&page.scrolled, Some(&key));
        }
        self.stack.set_visible_child_name(&key);
        let title = self
            .titles
            .borrow()
            .get(&key)
            .cloned()
            .unwrap_or_else(|| route.title());
        self.title.set_label(&title);
        self.title.set_visible(false);
        self.bind_scroll();
        self.select_sidebar(route);
    }

    fn bind_scroll(&self) {
        if let Some((adj, id)) = self.scroll.borrow_mut().take() {
            adj.disconnect(id);
        }
        let Some(child) = self
            .stack
            .visible_child()
            .and_then(|w| w.downcast::<gtk::ScrolledWindow>().ok())
        else {
            return;
        };
        let adj = child.vadjustment();
        let title = self.title.clone();
        let id = adj.connect_value_changed(move |adj| {
            title.set_visible(adj.value() > 36.0);
        });
        self.scroll.borrow_mut().replace((adj, id));
    }

    fn select_sidebar(&self, route: &Route) {
        self.guard.set(true);
        match route.sidebar() {
            Sidebar::Id(id) => {
                if let Some(index) = self.ids.iter().position(|s| *s == id) {
                    self.sidebar.select_row(self.rows.get(index));
                }
            }
            Sidebar::Clear => self.sidebar.unselect_all(),
            Sidebar::Keep => {}
        }
        self.guard.set(false);
    }

    fn submit_search(&self, text: String) {
        if text.is_empty() {
            return;
        }
        let route = Route::Search(text);
        if matches!(self.current.borrow().clone(), Route::Search(_)) {
            self.drop_page(&self.current.borrow().clone());
            self.current.replace(route.clone());
            self.present(&route);
            return;
        }
        // show() records history. Call through the same path.
        // We are &self, show needs Rc. Use nav.
        if let Some(nav) = self.nav.borrow().clone() {
            nav.go(route);
        }
    }

    fn go_back(self: &Rc<Self>) {
        let Some(prev) = self.history.borrow_mut().pop() else {
            return;
        };
        self.future.borrow_mut().push(self.current.borrow().clone());
        self.show(prev, false);
    }

    fn go_forward(self: &Rc<Self>) {
        let Some(next) = self.future.borrow_mut().pop() else {
            return;
        };
        self.history
            .borrow_mut()
            .push(self.current.borrow().clone());
        self.show(next, false);
    }

    fn sync_buttons(&self) {
        self.back.set_sensitive(!self.history.borrow().is_empty());
        self.forward.set_sensitive(!self.future.borrow().is_empty());
    }

    fn clear_search(&self) {
        if self.search.text().is_empty() {
            return;
        }
        self.guard.set(true);
        self.search.set_text("");
        self.guard.set(false);
    }

    fn drop_page(&self, route: &Route) {
        if let Some(child) = self.stack.child_by_name(&route.key()) {
            self.stack.remove(&child);
        }
    }

    fn ask(&self, text: String, yes: Box<dyn FnOnce()>) {
        self.confirm_label.set_label(&text);
        *self.confirm_yes.borrow_mut() = Some(yes);
        self.confirm_window.present();
    }
}

fn sidebar_items() -> Vec<SidebarItem> {
    vec![
        SidebarItem {
            id: "discover",
            icon: "starred-symbolic",
            title: || tr("Discover"),
            badge: false,
        },
        SidebarItem {
            id: "create",
            icon: "applications-graphics-symbolic",
            title: || tr("Create"),
            badge: false,
        },
        SidebarItem {
            id: "work",
            icon: "x-office-document-symbolic",
            title: || tr("Work"),
            badge: false,
        },
        SidebarItem {
            id: "play",
            icon: "applications-games-symbolic",
            title: || tr("Play"),
            badge: false,
        },
        SidebarItem {
            id: "develop",
            icon: "applications-engineering-symbolic",
            title: || tr("Develop"),
            badge: false,
        },
        SidebarItem {
            id: "categories",
            icon: "view-grid-symbolic",
            title: || tr("Categories"),
            badge: false,
        },
        SidebarItem {
            id: "updates",
            icon: "software-update-available-symbolic",
            title: || tr("Updates"),
            badge: true,
        },
        SidebarItem {
            id: "installed",
            icon: "package-x-generic-symbolic",
            title: || tr("Installed"),
            badge: false,
        },
    ]
}

fn sidebar_row(item: &SidebarItem, badge: &gtk::Label) -> gtk::ListBoxRow {
    let content = gtk::Box::builder().spacing(8).build();
    content.append(
        &gtk::Image::builder()
            .icon_name(item.icon)
            .pixel_size(16)
            .build(),
    );
    content.append(
        &gtk::Label::builder()
            .label((item.title)())
            .xalign(0.0)
            .hexpand(true)
            .build(),
    );
    if item.badge {
        content.append(badge);
    }
    gtk::ListBoxRow::builder()
        .child(&content)
        .name(item.id)
        .build()
}

fn nav_button(icon: &str, tooltip: &str) -> gtk::Button {
    gtk::Button::builder()
        .child(&gtk::Image::builder().icon_name(icon).pixel_size(14).build())
        .tooltip_text(tooltip)
        .sensitive(false)
        .css_classes(["flat"])
        .build()
}

fn confirm_dialog(parent: &gtk::ApplicationWindow) -> (gtk::Window, gtk::Label, Pending) {
    let window = gtk::Window::builder()
        .transient_for(parent)
        .modal(true)
        .title(tr("App Bay"))
        .default_width(360)
        .hide_on_close(true)
        .build();
    let label = gtk::Label::builder()
        .wrap(true)
        .margin_top(20)
        .margin_bottom(12)
        .margin_start(20)
        .margin_end(20)
        .build();
    let yes = gtk::Button::builder()
        .label(tr("Uninstall"))
        .css_classes(["pill"])
        .build();
    let no = gtk::Button::builder()
        .label(tr("Cancel"))
        .css_classes(["flat"])
        .build();
    let buttons = gtk::Box::builder()
        .spacing(8)
        .halign(gtk::Align::End)
        .margin_bottom(16)
        .margin_end(16)
        .build();
    buttons.append(&no);
    buttons.append(&yes);
    let column = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .build();
    column.append(&label);
    column.append(&buttons);
    window.set_child(Some(&column));
    let pending: Pending = Rc::new(RefCell::new(None));
    let pending_yes = pending.clone();
    let window_yes = window.clone();
    yes.connect_clicked(move |_| {
        window_yes.set_visible(false);
        if let Some(run) = pending_yes.borrow_mut().take() {
            run();
        }
    });
    let window_no = window.clone();
    let pending_no = pending.clone();
    no.connect_clicked(move |_| {
        pending_no.borrow_mut().take();
        window_no.set_visible(false);
    });
    (window, label, pending)
}
