//! One instance. The menubar is exported for the panel and not drawn.

use std::cell::RefCell;
use std::rc::Rc;

use gtk::prelude::*;
use gtk::{gdk, gio, glib};

use crate::context::Ctx;
use crate::i18n::tr;
use crate::nav::Route;
use crate::window::Window;

thread_local! {
    static WINDOW: RefCell<Option<Rc<Window>>> = const { RefCell::new(None) };
}

pub fn run() -> glib::ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!("Usage: elavo-app-bay");
        return glib::ExitCode::SUCCESS;
    }
    let app = gtk::Application::builder()
        .application_id(crate::APP_ID)
        .build();
    app.connect_startup(|app| {
        load_css();
        app.set_menubar(Some(&menubar()));
    });
    app.connect_activate(activate);
    let discover = gio::SimpleAction::new("discover", None);
    discover.connect_activate(|_, _| {
        if let Some(window) = WINDOW.with(|w| w.borrow().clone()) {
            window.nav().go(Route::Discover);
        }
    });
    app.add_action(&discover);
    app.run()
}

fn activate(app: &gtk::Application) {
    if let Some(window) = WINDOW.with(|w| w.borrow().clone()) {
        window.window.present();
        return;
    }
    let ctx = Ctx::new();
    let window = Window::new(app, ctx.clone());
    window.window.set_hide_on_close(true);
    let app_hide = app.clone();
    window.window.connect_hide(move |_| app_hide.quit());
    WINDOW.with(|cell| cell.replace(Some(window.clone())));
    window.window.present();
    glib::spawn_future_local(async move {
        ctx.refresh_remote().await;
        ctx.refresh_installed().await;
        window.refresh_badge();
        window.reload();
    });
}

fn load_css() {
    let Some(display) = gdk::Display::default() else {
        return;
    };
    let css = gtk::CssProvider::new();
    css.load_from_resource("/io/elavo/AppBay/style.css");
    gtk::style_context_add_provider_for_display(
        &display,
        &css,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
}

fn menubar() -> gio::Menu {
    let menu = gio::Menu::new();
    let view = gio::Menu::new();
    view.append(Some(&tr("Discover")), Some("app.discover"));
    menu.append_section(None, &view);
    menu
}
