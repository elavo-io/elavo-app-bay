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
use crate::task::{Job, Phase};

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

/// A spinner and a line, shown while a list is still coming in.
pub fn loading(text: &str) -> gtk::Box {
    let row = gtk::Box::builder()
        .spacing(10)
        .halign(gtk::Align::Start)
        .margin_top(8)
        .margin_bottom(8)
        .build();
    let spinner = gtk::Spinner::builder()
        .spinning(true)
        .width_request(16)
        .height_request(16)
        .build();
    row.append(&spinner);
    row.append(
        &gtk::Label::builder()
            .label(text)
            .css_classes(["caption"])
            .build(),
    );
    row
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

pub fn app_row(brief: &Brief, numbered: Option<usize>, button: &impl IsA<gtk::Widget>) -> gtk::Box {
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

/// The Get / Open / Update control, with a bar under the pill while work
/// is running and the failure in words when it stops.
pub struct ActionControl {
    pub root: gtk::Box,
    button: gtk::Button,
    label: gtk::Label,
    bar: gtk::ProgressBar,
    error: gtk::Label,
    pulse: RefCell<Option<glib::SourceId>>,
}

impl ActionControl {
    pub fn new(text: &str) -> Self {
        let label = gtk::Label::builder().label(text).build();
        let button = gtk::Button::builder()
            .child(&label)
            .css_classes(["pill"])
            .valign(gtk::Align::Center)
            .build();
        let bar = gtk::ProgressBar::builder()
            .valign(gtk::Align::Center)
            .width_request(112)
            .visible(false)
            .build();
        bar.set_pulse_step(0.08);
        let error = gtk::Label::builder()
            .wrap(true)
            .wrap_mode(gtk::pango::WrapMode::WordChar)
            .justify(gtk::Justification::Center)
            .max_width_chars(22)
            .visible(false)
            .css_classes(["bay-action-error"])
            .build();
        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(4)
            .valign(gtk::Align::Center)
            .halign(gtk::Align::End)
            .css_classes(["bay-action"])
            .build();
        root.append(&button);
        root.append(&bar);
        root.append(&error);
        Self {
            root,
            button,
            label,
            bar,
            error,
            pulse: RefCell::new(None),
        }
    }

    pub fn connect_clicked(&self, f: impl Fn(&gtk::Button) + 'static) {
        self.button.connect_clicked(f);
    }

    pub fn show_offer(&self, offer: Offer) {
        self.stop_pulse();
        self.bar.set_visible(false);
        self.bar.set_fraction(0.0);
        self.error.set_visible(false);
        self.label.set_label(&offer_label(offer));
        self.button.remove_css_class("failed");
        self.button.set_tooltip_text(None);
    }

    pub fn show_job(&self, job: &Job) {
        match job {
            Job::Working {
                phase, fraction, ..
            } => {
                self.error.set_visible(false);
                self.button.remove_css_class("failed");
                self.button.set_tooltip_text(None);
                self.label.set_label(&working_text(*phase, *fraction));
                self.bar.set_visible(true);
                if let Some(fraction) = fraction {
                    self.stop_pulse();
                    self.bar.set_fraction(*fraction);
                } else if self.pulse.borrow().is_none() {
                    let bar = self.bar.clone();
                    let id =
                        glib::timeout_add_local(std::time::Duration::from_millis(120), move || {
                            bar.pulse();
                            glib::ControlFlow::Continue
                        });
                    *self.pulse.borrow_mut() = Some(id);
                }
            }
            Job::Failed { message } => {
                self.stop_pulse();
                self.bar.set_visible(false);
                self.label.set_label(&tr("Try Again"));
                self.button.add_css_class("failed");
                self.error.set_label(message);
                self.error.set_visible(true);
                self.button.set_tooltip_text(Some(message));
            }
        }
    }

    fn stop_pulse(&self) {
        if let Some(id) = self.pulse.borrow_mut().take() {
            id.remove();
        }
    }
}

impl Drop for ActionControl {
    fn drop(&mut self) {
        self.stop_pulse();
    }
}

pub fn working_text(phase: Phase, fraction: Option<f64>) -> String {
    let name = match phase {
        Phase::Preparing => tr("Preparing…"),
        Phase::Waiting => tr("Waiting…"),
        Phase::Downloading => tr("Downloading…"),
        Phase::Installing => tr("Installing…"),
        Phase::Updating => tr("Updating…"),
        Phase::Finishing => tr("Finishing…"),
    };
    match fraction {
        Some(fraction) => {
            let percent = (fraction.clamp(0.0, 1.0) * 100.0).round() as i32;
            let name = name.trim_end_matches('…');
            format!("{name} {percent}%")
        }
        None => name,
    }
}

pub fn offer_label(offer: Offer) -> String {
    match offer {
        Offer::Get => tr("Get"),
        Offer::Open => tr("Open"),
        Offer::Update => tr("Update"),
    }
}

/// A square icon slot. The pixel size is the widget's size, so a 256px file
/// draws at the same size as a 128px one. The frame clips it to the rounded
/// square; the image alone would keep its own corners.
pub fn icon_picture(size: i32) -> gtk::Box {
    let image = gtk::Image::builder()
        .pixel_size(size)
        .hexpand(true)
        .vexpand(true)
        .build();
    let slot = gtk::Box::builder()
        .width_request(size)
        .height_request(size)
        .halign(gtk::Align::Start)
        .valign(gtk::Align::Center)
        .overflow(gtk::Overflow::Hidden)
        .css_classes(["bay-icon"])
        .build();
    slot.append(&image);
    slot
}

pub fn load_picture(http: &Http, widget: &impl IsA<gtk::Widget>, url: &str) {
    if url.is_empty() {
        return;
    }
    let widget = widget.upcast_ref::<gtk::Widget>().clone();
    if let Some(path) = local_path(url) {
        show_file(&widget, &path);
        return;
    }
    let http = http.clone();
    let url = url.to_owned();
    glib::spawn_future_local(async move {
        let Ok(bytes) = http.get(&url, 24 * 60 * 60).await else {
            return;
        };
        let gbytes = glib::Bytes::from(&bytes);
        if let Ok(texture) = gdk::Texture::from_bytes(&gbytes) {
            show_paintable(&widget, &texture);
        }
    });
}

/// Remember the URL on the widget. Call `flush_pictures` once an `Http` exists.
fn load_picture_later(widget: &impl IsA<gtk::Widget>, url: &str) {
    let widget = widget.upcast_ref::<gtk::Widget>();
    if let Some(path) = local_path(url) {
        show_file(widget, &path);
        return;
    }
    unsafe {
        widget.set_data("bay-url", Rc::new(url.to_owned()));
    }
}

fn show_file(widget: &gtk::Widget, path: &str) {
    if let Some(picture) = widget.downcast_ref::<gtk::Picture>() {
        picture.set_filename(Some(path));
    } else if let Some(image) = icon_image(widget) {
        image.set_from_file(Some(path));
    }
}

fn show_paintable(widget: &gtk::Widget, texture: &gdk::Texture) {
    if let Some(picture) = widget.downcast_ref::<gtk::Picture>() {
        picture.set_paintable(Some(texture));
    } else if let Some(image) = icon_image(widget) {
        image.set_paintable(Some(texture));
    }
}

fn icon_image(widget: &gtk::Widget) -> Option<gtk::Image> {
    if let Some(image) = widget.downcast_ref::<gtk::Image>() {
        return Some(image.clone());
    }
    widget
        .first_child()
        .and_then(|child| child.downcast::<gtk::Image>().ok())
}

pub fn flush_pictures(root: &impl IsA<gtk::Widget>, http: &Http) {
    let http = http.clone();
    walk(root.upcast_ref(), &http);
}

fn walk(widget: &gtk::Widget, http: &Http) {
    let url = unsafe {
        widget
            .data::<Rc<String>>("bay-url")
            .map(|p| p.as_ref().clone())
    };
    if let Some(url) = url.filter(|url| !url.is_empty()) {
        unsafe { widget.set_data("bay-url", Rc::new(String::new())) };
        load_picture(http, widget, &url);
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
