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
        follow_color_scheme();
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

/// `org.gnome.desktop.interface` `color-scheme`. Anything else is light,
/// including an empty value and `prefer-light`.
fn scheme_is_dark(value: &str) -> bool {
    value == "prefer-dark"
}

/// Load the dark theme sheet while the color scheme is dark, and switch
/// when it changes. GTK keeps the theme name `Elavo` and reads
/// `gtk-dark.css` instead of `gtk.css`. The settings object is kept so the
/// signal stays connected.
fn follow_color_scheme() {
    let Some(source) = gio::SettingsSchemaSource::default() else {
        return;
    };
    let Some(schema) = source.lookup("org.gnome.desktop.interface", true) else {
        return;
    };
    if !schema.has_key("color-scheme") {
        return;
    }
    let desktop = gio::Settings::new("org.gnome.desktop.interface");
    let apply = {
        let desktop = desktop.clone();
        move || {
            let Some(settings) = gtk::Settings::default() else {
                return;
            };
            settings.set_gtk_application_prefer_dark_theme(scheme_is_dark(
                desktop.string("color-scheme").as_str(),
            ));
        }
    };
    apply();
    desktop.connect_changed(Some("color-scheme"), move |_, _| apply());
    thread_local! {
        static HELD: RefCell<Option<gio::Settings>> = const { RefCell::new(None) };
    }
    HELD.with(|slot| slot.replace(Some(desktop)));
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

#[cfg(test)]
mod tests {
    use super::scheme_is_dark;

    #[test]
    fn only_prefer_dark_selects_the_dark_sheet() {
        assert!(scheme_is_dark("prefer-dark"));
        assert!(!scheme_is_dark("prefer-light"));
        assert!(!scheme_is_dark("default"));
        assert!(!scheme_is_dark(""));
    }
}
