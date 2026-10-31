//! Cards, rows, and the scrolling page. The window chrome lives in `window`.

use std::cell::RefCell;
use std::rc::Rc;

use gtk::gdk;
use gtk::prelude::*;
use gtk::{gio, glib};

thread_local! {
    static TINTED: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

use crate::http::Http;
use crate::i18n::tr;
use crate::model::{Brief, Offer, Story};

pub struct Page {
    pub scrolled: gtk::ScrolledWindow,
    pub title: String,
}

pub fn page(title: &str, end: Option<&gtk::Widget>, fill: impl FnOnce(&gtk::Box)) -> Page {
    let column = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(16)
        .css_classes(["bay-page"])
        .build();
    let heading = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    heading.append(
        &gtk::Label::builder()
            .label(title)
            .xalign(0.0)
            .hexpand(true)
            .css_classes(["bay-title"])
            .build(),
    );
    if let Some(end) = end {
        heading.append(end);
    }
    column.append(&heading);
    fill(&column);
    let scrolled = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .child(&column)
        .hexpand(true)
        .vexpand(true)
        .build();
    Page {
        scrolled,
        title: title.to_owned(),
    }
}

pub fn section(title: &str, see_all: Option<gtk::Button>) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    row.append(
        &gtk::Label::builder()
            .label(title)
            .xalign(0.0)
            .hexpand(true)
            .css_classes(["bay-section"])
            .build(),
    );
    if let Some(button) = see_all {
        button.add_css_class("linkish");
        button.add_css_class("flat");
        row.append(&button);
    }
    row
}

pub fn see_all() -> gtk::Button {
    gtk::Button::builder()
        .label(tr("See All"))
        .css_classes(["linkish", "flat"])
        .build()
}

pub fn note(text: &str) -> gtk::Label {
    gtk::Label::builder()
        .label(text)
        .xalign(0.0)
        .wrap(true)
        .css_classes(["caption"])
        .build()
}

pub fn story_button(story: &Story, image: &str, hero: bool) -> gtk::Button {
    let text = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(6)
        .valign(gtk::Align::Center)
        .hexpand(true)
        .build();
    if !story.kicker.is_empty() {
        text.append(
            &gtk::Label::builder()
                .label(&story.kicker)
                .xalign(0.0)
                .css_classes(["bay-kicker"])
                .build(),
        );
    }
    text.append(
        &gtk::Label::builder()
            .label(&story.title)
            .xalign(0.0)
            .wrap(true)
            .css_classes([if hero {
                "bay-hero-title"
            } else {
                "bay-card-title"
            }])
            .build(),
    );
    if !story.subtitle.is_empty() {
        text.append(
            &gtk::Label::builder()
                .label(&story.subtitle)
                .xalign(0.0)
                .wrap(true)
                .css_classes(["bay-subtitle"])
                .build(),
        );
    }
    let inner = gtk::Box::builder()
        .spacing(12)
        .css_classes(if hero { ["story-hero"] } else { ["story-card"] })
        .build();
    if let Some(color) = safe_color(&story.color) {
        tint(inner.upcast_ref(), color);
        inner.add_css_class("story-tinted");
    }
    inner.append(&text);
    if !image.is_empty() {
        let picture = gtk::Picture::builder()
            .width_request(if hero { 280 } else { 96 })
            .height_request(if hero { 180 } else { 96 })
            .content_fit(gtk::ContentFit::Cover)
            .css_classes(["story-art"])
            .build();
        load_picture_later(&picture, image);
        inner.append(&picture);
    }
    gtk::Button::builder()
        .child(&inner)
        .css_classes(["flat", "story-button"])
        .hexpand(true)
        .build()
}

pub fn story_row(cards: &[gtk::Widget]) -> gtk::Box {
    let row = gtk::Box::builder().spacing(12).homogeneous(true).build();
    for card in cards {
        row.append(card);
    }
    row
}

pub fn app_row(brief: &Brief, numbered: Option<usize>, button: &gtk::Button) -> gtk::Box {
    let row = gtk::Box::builder()
        .spacing(12)
        .css_classes(["bay-row"])
        .build();
    if let Some(n) = numbered {
        row.append(
            &gtk::Label::builder()
                .label(n.to_string())
                .css_classes(["bay-rank"])
                .build(),
        );
    }
    let icon = icon_picture(48);
    load_picture_later(&icon, &brief.icon);
    row.append(&icon);
    let text = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .valign(gtk::Align::Center)
        .hexpand(true)
        .build();
    text.append(
        &gtk::Label::builder()
            .label(&brief.name)
            .xalign(0.0)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .css_classes(["bay-app-name"])
            .build(),
    );
    if !brief.summary.is_empty() {
        text.append(
            &gtk::Label::builder()
                .label(&brief.summary)
                .xalign(0.0)
                .ellipsize(gtk::pango::EllipsizeMode::End)
                .css_classes(["caption"])
                .build(),
        );
    }
    row.append(&text);
    row.append(button);
    row
}

pub fn columns(rows: &[gtk::Widget]) -> gtk::Grid {
    let grid = gtk::Grid::builder()
        .column_spacing(28)
        .row_spacing(4)
        .column_homogeneous(true)
        .build();
    let mid = rows.len().div_ceil(2);
    for (i, row) in rows.iter().enumerate() {
        let (col, at): (i32, i32) = if i < mid {
            (0, i as i32)
        } else {
            (1, (i - mid) as i32)
        };
        grid.attach(row, col, at, 1, 1);
    }
    grid
}

pub fn pill(label: &str) -> gtk::Button {
    gtk::Button::builder()
        .label(label)
        .css_classes(["pill"])
        .valign(gtk::Align::Center)
        .build()
}

pub fn offer_label(offer: Offer) -> String {
    match offer {
        Offer::Get => tr("Get"),
        Offer::Open => tr("Open"),
        Offer::Update => tr("Update"),
    }
}

pub fn icon_picture(size: i32) -> gtk::Picture {
    gtk::Picture::builder()
        .width_request(size)
        .height_request(size)
        .content_fit(gtk::ContentFit::Cover)
        .css_classes(["bay-icon"])
        .build()
}

pub fn load_picture(http: &Http, picture: &gtk::Picture, url: &str) {
    if url.is_empty() {
        return;
    }
    if let Some(path) = local_path(url) {
        picture.set_filename(Some(&path));
        return;
    }
    let http = http.clone();
    let picture = picture.clone();
    let url = url.to_owned();
    glib::spawn_future_local(async move {
        let Ok(bytes) = http.get(&url, 24 * 60 * 60).await else {
            return;
        };
        let gbytes = glib::Bytes::from(&bytes);
        if let Ok(texture) = gdk::Texture::from_bytes(&gbytes) {
            picture.set_paintable(Some(&texture));
        }
    });
}

/// Remember the URL on the picture. Call `flush_pictures` once an `Http` exists.
fn load_picture_later(picture: &gtk::Picture, url: &str) {
    if let Some(path) = local_path(url) {
        picture.set_filename(Some(&path));
        return;
    }
    unsafe {
        picture.set_data("bay-url", Rc::new(url.to_owned()));
    }
}

pub fn flush_pictures(root: &impl IsA<gtk::Widget>, http: &Http) {
    let http = http.clone();
    walk(root.upcast_ref(), &http);
}

fn walk(widget: &gtk::Widget, http: &Http) {
    if let Some(picture) = widget.downcast_ref::<gtk::Picture>() {
        let url = unsafe {
            picture
                .data::<Rc<String>>("bay-url")
                .map(|p| p.as_ref().clone())
        };
        if let Some(url) = url {
            unsafe { picture.set_data("bay-url", Rc::new(String::new())) };
            load_picture(http, picture, &url);
        }
    }
    let mut child = widget.first_child();
    while let Some(next) = child {
        walk(&next, http);
        child = next.next_sibling();
    }
}

fn local_path(url: &str) -> Option<String> {
    if let Some(path) = url.strip_prefix("file://") {
        return Some(path.to_owned());
    }
    if url.starts_with("http://") || url.starts_with("https://") {
        return None;
    }
    if std::path::Path::new(url).exists() {
        return Some(url.to_owned());
    }
    None
}

fn safe_color(color: &str) -> Option<&str> {
    let bytes = color.as_bytes();
    if bytes.len() == 7 && bytes[0] == b'#' && bytes[1..].iter().all(|b| b.is_ascii_hexdigit()) {
        Some(color)
    } else {
        None
    }
}

fn tint(widget: &gtk::Widget, color: &str) {
    let class = format!("tint-{}", &color[1..]);
    widget.add_css_class(&class);
    TINTED.with(|tinted| {
        let mut tinted = tinted.borrow_mut();
        if tinted.iter().any(|c| c == &class) {
            return;
        }
        let css = gtk::CssProvider::new();
        css.load_from_string(&format!(".{class} {{ background-color: {color}; }}"));
        if let Some(display) = gdk::Display::default() {
            gtk::style_context_add_provider_for_display(
                &display,
                &css,
                gtk::STYLE_PROVIDER_PRIORITY_APPLICATION + 1,
            );
        }
        tinted.push(class);
    });
}

pub fn open_url(url: &str) {
    let _ = gio::AppInfo::launch_default_for_uri(url, None::<&gio::AppLaunchContext>);
}

pub fn paragraph(text: &str) -> gtk::Label {
    gtk::Label::builder()
        .label(text)
        .xalign(0.0)
        .wrap(true)
        .wrap_mode(gtk::pango::WrapMode::WordChar)
        .selectable(true)
        .build()
}
