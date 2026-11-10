mod app;
mod catalog;
mod categories;
mod context;
mod flathub;
mod http;
mod i18n;
mod install;
mod installed;
mod launch;
mod model;
mod nav;
mod pages;
mod process;
mod task;
mod text;
mod widgets;
mod window;

use gtk::glib;

pub const APP_ID: &str = "io.elavo.AppBay";

fn main() -> glib::ExitCode {
    i18n::init();
    gtk::gio::resources_register_include!("elavo-app-bay.gresource").expect("compiled resources");
    app::run()
}
