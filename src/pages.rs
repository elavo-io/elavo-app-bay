//! Discover, the hubs, categories, an app, and the installed list.

use std::rc::Rc;

use gtk::glib;
use gtk::prelude::*;

use crate::categories;
use crate::context::Ctx;
use crate::flathub::{self, Detail, Hit};
use crate::i18n::tr;
use crate::install;
use crate::model::{self, AppId, Brief, Offer, Rail};
use crate::nav::{Chart, Nav, Route};
use crate::text::{self, Permission};
use crate::widgets::{self, Page};

pub fn build(route: &Route, ctx: &Rc<Ctx>, nav: &Nav) -> Page {
    match route {
        Route::Discover => discover(ctx, nav),
        Route::Create => hub(ctx, nav, "create", &tr("Create")),
        Route::Work => hub(ctx, nav, "work", &tr("Work")),
        Route::Play => hub(ctx, nav, "play", &tr("Play")),
        Route::Develop => hub(ctx, nav, "develop", &tr("Develop")),
        Route::Categories => categories_page(ctx, nav),
        Route::Category(id) => category_page(ctx, nav, id),
        Route::Updates => updates(ctx, nav),
        Route::Installed => installed(ctx, nav),
        Route::Article(id) => article(ctx, nav, id),
        Route::App(id) => app_page(ctx, nav, id),
        Route::Search(query) => search(ctx, nav, query),
        Route::Chart(chart) => chart_page(ctx, nav, chart),
    }
}

fn discover(ctx: &Rc<Ctx>, nav: &Nav) -> Page {
    let ctx = ctx.clone();
    let nav = nav.clone();
    widgets::page(&tr("Discover"), None, |column| {
        let catalog = ctx.catalog.borrow().clone();
        append_stories(column, &catalog, "discover", &nav);
        let slot = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(18)
            .build();
        column.append(&slot);
        widgets::flush_pictures(column, &ctx.http);
        let ctx = ctx.clone();
        let nav = nav.clone();
        glib::spawn_future_local(async move {
            append_ranking(
                &slot,
                &ctx,
                &nav,
                &tr("Popular"),
                flathub::popular(&ctx.http).await,
                Route::Chart(Chart::Popular),
            )
            .await;
            append_ranking(
                &slot,
                &ctx,
                &nav,
                &tr("New Apps"),
                flathub::recently_added(&ctx.http).await,
                Route::Chart(Chart::New),
            )
            .await;
            append_rails(&slot, &ctx, &nav, "discover").await;
        });
    })
}

fn hub(ctx: &Rc<Ctx>, nav: &Nav, id: &'static str, title: &str) -> Page {
    let ctx = ctx.clone();
    let nav = nav.clone();
    let hub_id = id.to_owned();
    widgets::page(title, None, move |column| {
        let catalog = ctx.catalog.borrow().clone();
        append_stories(column, &catalog, id, &nav);
        let slot = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(18)
            .build();
        column.append(&slot);
        widgets::flush_pictures(column, &ctx.http);
        let ctx = ctx.clone();
        let nav = nav.clone();
        glib::spawn_future_local(async move {
            let slugs = categories::hub_slugs(&hub_id);
            let hits = flathub::merged(&ctx.http, slugs).await;
            append_ranking(
                &slot,
                &ctx,
                &nav,
                &tr("Popular"),
                hits,
                Route::Chart(Chart::Hub(hub_id.clone())),
            )
            .await;
            append_rails(&slot, &ctx, &nav, &hub_id).await;
        });
    })
}

fn categories_page(ctx: &Rc<Ctx>, nav: &Nav) -> Page {
    let ctx = ctx.clone();
    let nav = nav.clone();
    widgets::page(&tr("Categories"), None, move |column| {
        let grid = gtk::Grid::builder()
            .column_spacing(24)
            .row_spacing(0)
            .column_homogeneous(true)
            .build();
        for (i, category) in categories::all().iter().enumerate() {
            let button = category_button(category.icon, &categories::title(category.id));
            let nav = nav.clone();
            let hub = category.hub;
            let id = category.id.to_owned();
            button.connect_clicked(move |_| {
                if let Some(hub) = hub {
                    nav.go(Route::from_sidebar(hub));
                } else {
                    nav.go(Route::Category(id.clone()));
                }
            });
            grid.attach(&button, (i % 3) as i32, (i / 3) as i32, 1, 1);
        }
        column.append(&grid);
        let slot = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(12)
            .build();
        column.append(&slot);
        let ctx = ctx.clone();
        let nav = nav.clone();
        glib::spawn_future_local(async move {
            append_rails(&slot, &ctx, &nav, "categories").await;
        });
    })
}

fn category_page(ctx: &Rc<Ctx>, nav: &Nav, id: &str) -> Page {
    let ctx = ctx.clone();
    let nav = nav.clone();
    let id = id.to_owned();
    let title = categories::title(&id);
    widgets::page(&title, None, move |column| {
        let catalog = ctx.catalog.borrow().clone();
        append_stories(column, &catalog, &id, &nav);
        let curated = curated_in(&ctx, |app| app.categories.iter().any(|c| c == &id));
        if !curated.is_empty() {
            column.append(&widgets::section(&tr("From Elavo"), None));
            let block = brief_list(&ctx, &nav, &curated, false);
            column.append(&block);
            widgets::flush_pictures(&block, &ctx.http);
        }
        let slot = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(18)
            .build();
        column.append(&slot);
        widgets::flush_pictures(column, &ctx.http);
        let ctx = ctx.clone();
        let nav = nav.clone();
        let id = id.clone();
        glib::spawn_future_local(async move {
            append_rails(&slot, &ctx, &nav, &id).await;
            let hits = match categories::get(&id).and_then(|c| c.flathub) {
                Some(slug) => flathub::category(&ctx.http, slug).await,
                None => flathub::search(&ctx.http, &categories::title(&id)).await,
            };
            append_ranking(
                &slot,
                &ctx,
                &nav,
                &tr("Popular"),
                hits,
                Route::Chart(Chart::Category(id)),
            )
            .await;
        });
    })
}

fn chart_page(ctx: &Rc<Ctx>, nav: &Nav, chart: &Chart) -> Page {
    let ctx = ctx.clone();
    let nav = nav.clone();
    let chart = chart.clone();
    let title = chart.title();
    widgets::page(&title, None, move |column| {
        let slot = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(8)
            .build();
        column.append(&slot);
        glib::spawn_future_local(async move {
            let result = match &chart {
                Chart::Popular => flathub::popular(&ctx.http).await,
                Chart::New => flathub::recently_added(&ctx.http).await,
                Chart::Hub(id) => flathub::merged(&ctx.http, categories::hub_slugs(id)).await,
                Chart::Category(id) => match categories::get(id).and_then(|c| c.flathub) {
                    Some(slug) => flathub::category(&ctx.http, slug).await,
                    None => flathub::search(&ctx.http, &categories::title(id)).await,
                },
            };
            match result {
                Ok(hits) => {
                    let block = hit_list(&ctx, &nav, &hits, 60, true);
                    slot.append(&block);
                    widgets::flush_pictures(&block, &ctx.http);
                }
                Err(err) => slot.append(&widgets::note(&err)),
            }
        });
    })
}

fn search(ctx: &Rc<Ctx>, nav: &Nav, query: &str) -> Page {
    let ctx = ctx.clone();
    let nav = nav.clone();
    let query = query.to_owned();
    let title = query.clone();
    widgets::page(&title, None, move |column| {
        let local = local_matches(&ctx, &query);
        if !local.is_empty() {
            column.append(&widgets::section(&tr("From Elavo"), None));
            let block = brief_list(&ctx, &nav, &local, false);
            column.append(&block);
            widgets::flush_pictures(&block, &ctx.http);
        }
        let slot = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(8)
            .build();
        column.append(&slot);
        let query = query.clone();
        glib::spawn_future_local(async move {
            match flathub::search(&ctx.http, &query).await {
                Ok(hits) if !hits.is_empty() => {
                    slot.append(&widgets::section(&tr("Flathub"), None));
                    let block = hit_list(&ctx, &nav, &hits, 40, false);
                    slot.append(&block);
                    widgets::flush_pictures(&block, &ctx.http);
                }
                Ok(_) if local.is_empty() => slot.append(&widgets::note(&tr("No matching apps."))),
                Ok(_) => {}
                Err(err) => slot.append(&widgets::note(&err)),
            }
        });
    })
}

fn article(ctx: &Rc<Ctx>, nav: &Nav, id: &str) -> Page {
    let catalog = ctx.catalog.borrow().clone();
    let Some(story) = catalog.stories.iter().find(|s| s.id == id).cloned() else {
        return widgets::page(&tr("Story"), None, |column| {
            column.append(&widgets::note(&tr(
                "This story is no longer in the catalog.",
            )));
        });
    };
    let title = story.title.clone();
    let ctx = ctx.clone();
    let nav = nav.clone();
    widgets::page(&title, None, move |column| {
        if !story.kicker.is_empty() {
            column.append(
                &gtk::Label::builder()
                    .label(&story.kicker)
                    .xalign(0.0)
                    .css_classes(["bay-kicker"])
                    .build(),
            );
        }
        let image = catalog.resolve(&story.image);
        if !image.is_empty() {
            let picture = gtk::Picture::builder()
                .height_request(280)
                .content_fit(gtk::ContentFit::Cover)
                .css_classes(["story-art"])
                .build();
            widgets::load_picture(&ctx.http, &picture, &image);
            column.append(&picture);
        }
        if !story.date.is_empty() {
            column.append(&widgets::note(&story.date));
        }
        if !story.subtitle.is_empty() {
            column.append(&widgets::paragraph(&story.subtitle));
        }
        for para in story.body.split("\n\n") {
            let para = para.trim();
            if !para.is_empty() {
                column.append(&widgets::paragraph(para));
            }
        }
        let ids = story.apps.clone();
        let slot = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(8)
            .build();
        column.append(&slot);
        glib::spawn_future_local(async move {
            let briefs = resolve_ids(&ctx, &ids).await;
            if briefs.is_empty() {
                return;
            }
            slot.append(&widgets::section(&tr("Apps"), None));
            let block = brief_list(&ctx, &nav, &briefs, false);
            slot.append(&block);
            widgets::flush_pictures(&block, &ctx.http);
        });
    })
}

fn app_page(ctx: &Rc<Ctx>, nav: &Nav, id: &AppId) -> Page {
    let ctx = ctx.clone();
    let nav = nav.clone();
    let id = id.clone();
    let placeholder = match &id {
        AppId::Flathub(fid) => fid.clone(),
        AppId::AppImage(iid) | AppId::FlatpakRef(iid) => ctx
            .catalog
            .borrow()
            .curated(iid)
            .map(|a| a.name.clone())
            .unwrap_or_else(|| iid.clone()),
    };
    widgets::page(&placeholder, None, move |column| {
        let slot = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(12)
            .build();
        column.append(&slot);
        let nav = nav.clone();
        glib::spawn_future_local(async move {
            let built = match &id {
                AppId::Flathub(fid) => match flathub::detail(&ctx.http, fid).await {
                    Ok(detail) => Ok(detail_view(&ctx, &nav, &detail)),
                    Err(err) => Err(err),
                },
                AppId::AppImage(iid) | AppId::FlatpakRef(iid) => {
                    let Some(app) = ctx.catalog.borrow().curated(iid).cloned() else {
                        return slot
                            .append(&widgets::note(&tr("This app is no longer in the catalog.")));
                    };
                    let brief = brief_from_curated(&ctx, &app);
                    Ok(detail_view(
                        &ctx,
                        &nav,
                        &Detail {
                            brief,
                            description: String::new(),
                            homepage: app.homepage.clone(),
                            screenshots: Vec::new(),
                            release_version: app.version.clone(),
                            release_date: String::new(),
                            release_notes: String::new(),
                            download_size: None,
                            permissions: Vec::new(),
                        },
                    ))
                }
            };
            match built {
                Ok(view) => {
                    slot.append(&view);
                    widgets::flush_pictures(&view, &ctx.http);
                    nav.retitle(heading_of(&view));
                }
                Err(err) => slot.append(&widgets::note(&err)),
            }
        });
    })
}

fn updates(ctx: &Rc<Ctx>, nav: &Nav) -> Page {
    let ctx = ctx.clone();
    let nav = nav.clone();
    let update_all = gtk::Button::builder()
        .label(tr("Update All"))
        .css_classes(["linkish", "flat"])
        .build();
    let ctx_button = ctx.clone();
    let nav_button = nav.clone();
    update_all.connect_clicked(move |button| {
        button.set_sensitive(false);
        let ctx = ctx_button.clone();
        let nav = nav_button.clone();
        let button = button.clone();
        glib::spawn_future_local(async move {
            let _ = install::update_flatpak(None).await;
            ctx.refresh_installed().await;
            button.set_sensitive(true);
            nav.go(Route::Updates);
        });
    });
    widgets::page(
        &tr("Updates"),
        Some(update_all.upcast_ref()),
        move |column| {
            let slot = gtk::Box::builder()
                .orientation(gtk::Orientation::Vertical)
                .spacing(12)
                .build();
            column.append(&slot);
            glib::spawn_future_local(async move {
                ctx.refresh_installed().await;
                let installed = ctx.installed.borrow().clone();
                let mut cards: Vec<gtk::Widget> = Vec::new();
                for (id, version) in &installed.updates {
                    let name = installed
                        .flatpaks
                        .get(id)
                        .map(|a| a.name.clone())
                        .unwrap_or_else(|| id.clone());
                    let brief = Brief {
                        id: AppId::Flathub(id.clone()),
                        name,
                        summary: String::new(),
                        icon: String::new(),
                        developer: String::new(),
                        version: version.clone(),
                        download: String::new(),
                    };
                    cards.push(update_card(&ctx, &nav, &brief, "").upcast());
                }
                let versions = ctx.catalog_versions();
                for (id, app) in &installed.images {
                    let Some(version) = versions.get(id) else {
                        continue;
                    };
                    if version.is_empty() || app.version.is_empty() || app.version == *version {
                        continue;
                    }
                    let Some(curated) = ctx.catalog.borrow().curated(id).cloned() else {
                        continue;
                    };
                    let brief = brief_from_curated(&ctx, &curated);
                    cards.push(update_card(&ctx, &nav, &brief, "").upcast());
                }
                if cards.is_empty() {
                    slot.append(&widgets::note(&tr("Everything is up to date.")));
                    return;
                }
                slot.append(
                    &gtk::Label::builder()
                        .label(tr("Available"))
                        .xalign(0.0)
                        .css_classes(["bay-section"])
                        .build(),
                );
                let grid = widgets::columns(&cards);
                slot.append(&grid);
                widgets::flush_pictures(&grid, &ctx.http);
                fill_update_notes(&ctx, &grid).await;
            });
        },
    )
}

fn installed(ctx: &Rc<Ctx>, nav: &Nav) -> Page {
    let ctx = ctx.clone();
    let nav = nav.clone();
    widgets::page(&tr("Installed"), None, move |column| {
        let slot = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(4)
            .build();
        column.append(&slot);
        glib::spawn_future_local(async move {
            ctx.refresh_installed().await;
            let installed = ctx.installed.borrow().clone();
            if installed.flatpaks.is_empty() && installed.images.is_empty() {
                slot.append(&widgets::note(&tr("Nothing is installed yet.")));
                return;
            }
            let mut briefs = Vec::new();
            for (id, app) in &installed.flatpaks {
                briefs.push(Brief {
                    id: AppId::Flathub(id.clone()),
                    name: app.name.clone(),
                    summary: app.version.clone(),
                    icon: String::new(),
                    developer: String::new(),
                    version: app.version.clone(),
                    download: String::new(),
                });
            }
            for (id, app) in &installed.images {
                let mut brief = ctx
                    .catalog
                    .borrow()
                    .curated(id)
                    .cloned()
                    .map(|a| brief_from_curated(&ctx, &a))
                    .unwrap_or(Brief {
                        id: AppId::AppImage(id.clone()),
                        name: app.name.clone(),
                        summary: String::new(),
                        icon: String::new(),
                        developer: String::new(),
                        version: app.version.clone(),
                        download: String::new(),
                    });
                brief.summary = app.version.clone();
                briefs.push(brief);
            }
            briefs.sort_by_key(|a| a.name.to_lowercase());
            let block = brief_list(&ctx, &nav, &briefs, false);
            slot.append(&block);
            widgets::flush_pictures(&block, &ctx.http);
        });
    })
}

async fn append_ranking(
    slot: &gtk::Box,
    ctx: &Rc<Ctx>,
    nav: &Nav,
    title: &str,
    hits: Result<Vec<Hit>, String>,
    more: Route,
) {
    match hits {
        Ok(hits) if !hits.is_empty() => {
            let see = widgets::see_all();
            let nav_see = nav.clone();
            see.connect_clicked(move |_| nav_see.go(more.clone()));
            slot.append(&widgets::section(title, Some(see)));
            let shown: Vec<Hit> = hits.into_iter().take(6).collect();
            let block = hit_list(ctx, nav, &shown, 6, true);
            slot.append(&block);
            widgets::flush_pictures(&block, &ctx.http);
        }
        Ok(_) => {}
        Err(err) => slot.append(&widgets::note(&err)),
    }
}

async fn append_rails(slot: &gtk::Box, ctx: &Rc<Ctx>, nav: &Nav, surface: &str) {
    let rails: Vec<Rail> = model::rails_for(&ctx.catalog.borrow().rails, surface)
        .into_iter()
        .cloned()
        .collect();
    for rail in rails {
        let briefs = resolve_ids(ctx, &rail.apps).await;
        if briefs.is_empty() {
            continue;
        }
        slot.append(&widgets::section(&rail.title, None));
        let block = brief_list(ctx, nav, &briefs, false);
        slot.append(&block);
        widgets::flush_pictures(&block, &ctx.http);
    }
}

fn append_stories(column: &gtk::Box, catalog: &crate::catalog::Catalog, surface: &str, nav: &Nav) {
    let stories = model::stories_for(&catalog.stories, surface);
    let mut cards: Vec<gtk::Widget> = Vec::new();
    for (i, story) in stories.iter().enumerate() {
        let image = catalog.resolve(&story.image);
        let button = widgets::story_button(story, &image, i == 0);
        let nav = nav.clone();
        let id = story.id.clone();
        button.connect_clicked(move |_| nav.go(Route::Article(id.clone())));
        if i == 0 {
            column.append(&button);
        } else {
            cards.push(button.upcast());
            if cards.len() == 2 {
                column.append(&widgets::story_row(&std::mem::take(&mut cards)));
            }
        }
    }
    if !cards.is_empty() {
        column.append(&widgets::story_row(&cards));
    }
}

fn hit_list(ctx: &Rc<Ctx>, nav: &Nav, hits: &[Hit], limit: usize, numbered: bool) -> gtk::Grid {
    let rows: Vec<gtk::Widget> = hits
        .iter()
        .take(limit)
        .enumerate()
        .map(|(i, hit)| {
            let brief = hit.brief();
            let button = action_button(ctx, brief.clone());
            let row = widgets::app_row(&brief, numbered.then_some(i + 1), &button);
            open_on_click(&row, nav, brief.id);
            row.upcast()
        })
        .collect();
    widgets::columns(&rows)
}

fn brief_list(ctx: &Rc<Ctx>, nav: &Nav, briefs: &[Brief], numbered: bool) -> gtk::Grid {
    let rows: Vec<gtk::Widget> = briefs
        .iter()
        .enumerate()
        .map(|(i, brief)| {
            let button = action_button(ctx, brief.clone());
            let row = widgets::app_row(brief, numbered.then_some(i + 1), &button);
            open_on_click(&row, nav, brief.id.clone());
            row.upcast()
        })
        .collect();
    widgets::columns(&rows)
}

fn open_on_click(row: &gtk::Box, nav: &Nav, id: AppId) {
    let click = gtk::GestureClick::new();
    let nav = nav.clone();
    click.connect_released(move |_, _, _, _| nav.go(Route::App(id.clone())));
    row.add_controller(click);
}

fn action_button(ctx: &Rc<Ctx>, brief: Brief) -> gtk::Button {
    let offer = ctx.offer(&brief.id, &brief.version);
    let button = widgets::pill(&widgets::offer_label(offer));
    let ctx = ctx.clone();
    button.connect_clicked(move |button| {
        let offer = ctx.offer(&brief.id, &brief.version);
        let ctx = ctx.clone();
        let brief = brief.clone();
        let button = button.clone();
        match offer {
            Offer::Open => {
                let path = image_path(&ctx, &brief.id);
                let _ = install::open_app(&brief.id, path.as_deref());
            }
            Offer::Get | Offer::Update => {
                button.set_sensitive(false);
                if !matches!(brief.id, AppId::Flathub(_))
                    || offer == Offer::Update && matches!(brief.id, AppId::AppImage(_))
                {
                    button.set_label(&tr("Downloading…"));
                }
                glib::spawn_future_local(async move {
                    let result = perform(&ctx, &brief, offer).await;
                    button.set_sensitive(true);
                    if let Err(err) = result {
                        button.set_tooltip_text(Some(&err));
                    }
                    ctx.refresh_installed().await;
                    let offer = ctx.offer(&brief.id, &brief.version);
                    button.set_label(&widgets::offer_label(offer));
                });
            }
        }
    });
    button
}

async fn perform(ctx: &Ctx, brief: &Brief, offer: Offer) -> Result<(), String> {
    match (&brief.id, offer) {
        (AppId::Flathub(id), Offer::Get) => install::install_flathub(id, &brief.name).await,
        (AppId::Flathub(id) | AppId::FlatpakRef(id), Offer::Update) => {
            if ctx.installed.borrow().flatpaks.contains_key(id) {
                install::update_flatpak(Some(id)).await
            } else {
                download_curated(ctx, brief).await
            }
        }
        (AppId::FlatpakRef(_), Offer::Get) | (AppId::AppImage(_), Offer::Get | Offer::Update) => {
            download_curated(ctx, brief).await
        }
        _ => Ok(()),
    }
}

async fn download_curated(ctx: &Ctx, brief: &Brief) -> Result<(), String> {
    if brief.download.is_empty() {
        return Err(tr("No download is set for this app."));
    }
    let name = install::download_name(&brief.id, &brief.download);
    install::install_download(&ctx.http, &brief.download, &name).await
}

fn image_path(ctx: &Ctx, id: &AppId) -> Option<std::path::PathBuf> {
    let AppId::AppImage(iid) = id else {
        return None;
    };
    ctx.installed
        .borrow()
        .images
        .get(iid)
        .map(|a| a.path.clone())
}

fn curated_in(ctx: &Ctx, pred: impl Fn(&model::Curated) -> bool) -> Vec<Brief> {
    ctx.catalog
        .borrow()
        .apps
        .iter()
        .filter(|app| pred(app))
        .map(|app| brief_from_curated(ctx, app))
        .collect()
}

fn local_matches(ctx: &Ctx, query: &str) -> Vec<Brief> {
    let query = query.to_lowercase();
    ctx.catalog
        .borrow()
        .apps
        .iter()
        .filter(|app| {
            app.name.to_lowercase().contains(&query)
                || app.summary.to_lowercase().contains(&query)
                || app.developer.to_lowercase().contains(&query)
        })
        .map(|app| brief_from_curated(ctx, app))
        .collect()
}

fn brief_from_curated(ctx: &Ctx, app: &model::Curated) -> Brief {
    Brief {
        id: app.app_id(),
        name: app.name.clone(),
        summary: app.summary.clone(),
        icon: ctx.catalog.borrow().resolve(&app.icon),
        developer: app.developer.clone(),
        version: app.version.clone(),
        download: app.download.clone(),
    }
}

async fn resolve_ids(ctx: &Ctx, ids: &[String]) -> Vec<Brief> {
    let mut out = Vec::new();
    for raw in ids {
        let id = AppId::parse(raw);
        match &id {
            AppId::Flathub(fid) => {
                if let Ok(detail) = flathub::detail(&ctx.http, fid).await {
                    out.push(detail.brief);
                }
            }
            AppId::AppImage(iid) | AppId::FlatpakRef(iid) => {
                if let Some(app) = ctx.catalog.borrow().curated(iid).cloned() {
                    out.push(brief_from_curated(ctx, &app));
                }
            }
        }
    }
    out
}

fn category_button(icon: &str, title: &str) -> gtk::Button {
    let row = gtk::Box::builder().spacing(10).build();
    row.append(&gtk::Image::builder().icon_name(icon).pixel_size(22).build());
    row.append(
        &gtk::Label::builder()
            .label(title)
            .xalign(0.0)
            .hexpand(true)
            .build(),
    );
    gtk::Button::builder()
        .child(&row)
        .hexpand(true)
        .css_classes(["flat", "category-row"])
        .build()
}

fn detail_view(ctx: &Rc<Ctx>, nav: &Nav, detail: &Detail) -> gtk::Box {
    let brief = &detail.brief;
    let column = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(14)
        .build();
    let header = gtk::Box::builder().spacing(16).build();
    let icon = widgets::icon_picture(88);
    widgets::load_picture(&ctx.http, &icon, &brief.icon);
    header.append(&icon);
    let text = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(4)
        .hexpand(true)
        .valign(gtk::Align::Center)
        .build();
    text.append(
        &gtk::Label::builder()
            .label(&brief.name)
            .xalign(0.0)
            .css_classes(["bay-hero-title"])
            .name("bay-heading")
            .build(),
    );
    if !brief.summary.is_empty() {
        text.append(&widgets::paragraph(&brief.summary));
    }
    if !brief.developer.is_empty() {
        text.append(&widgets::note(&brief.developer));
    }
    header.append(&text);
    header.append(&action_button(ctx, brief.clone()));
    column.append(&header);
    if !detail.screenshots.is_empty() {
        let strip = gtk::Box::builder().spacing(10).build();
        for url in &detail.screenshots {
            let picture = gtk::Picture::builder()
                .width_request(360)
                .height_request(220)
                .content_fit(gtk::ContentFit::Cover)
                .css_classes(["story-art"])
                .build();
            widgets::load_picture(&ctx.http, &picture, url);
            strip.append(&picture);
        }
        column.append(
            &gtk::ScrolledWindow::builder()
                .child(&strip)
                .hscrollbar_policy(gtk::PolicyType::Automatic)
                .vscrollbar_policy(gtk::PolicyType::Never)
                .build(),
        );
    }
    if !detail.description.is_empty() {
        column.append(&widgets::paragraph(&detail.description));
    }
    if !detail.release_version.is_empty() || !detail.release_notes.is_empty() {
        let heading = if detail.release_date.is_empty() {
            tr("What's new")
        } else {
            format!("{} · {}", tr("What's new"), detail.release_date)
        };
        column.append(
            &gtk::Label::builder()
                .label(&heading)
                .xalign(0.0)
                .css_classes(["bay-section"])
                .build(),
        );
        if !detail.release_version.is_empty() {
            column.append(&widgets::note(&detail.release_version));
        }
        if !detail.release_notes.is_empty() {
            column.append(&widgets::paragraph(&detail.release_notes));
        }
    }
    if let Some(size) = detail.download_size {
        column.append(&widgets::note(&format!(
            "{} {}",
            tr("Download size"),
            text::format_size(size)
        )));
    }
    if !detail.permissions.is_empty() {
        column.append(
            &gtk::Label::builder()
                .label(tr("This app asks to"))
                .xalign(0.0)
                .css_classes(["bay-section"])
                .build(),
        );
        for permission in &detail.permissions {
            column.append(&widgets::note(&permission_text(permission)));
        }
    }
    if !detail.homepage.is_empty() {
        let button = gtk::Button::builder()
            .label(tr("Website"))
            .css_classes(["linkish", "flat"])
            .halign(gtk::Align::Start)
            .build();
        let homepage = detail.homepage.clone();
        button.connect_clicked(move |_| widgets::open_url(&homepage));
        column.append(&button);
    }
    if ctx.offer(&brief.id, &brief.version) != Offer::Get {
        let remove = gtk::Button::builder()
            .label(tr("Uninstall"))
            .css_classes(["flat"])
            .halign(gtk::Align::Start)
            .build();
        let ctx = ctx.clone();
        let nav = nav.clone();
        let brief = brief.clone();
        remove.connect_clicked(move |_| {
            let name = brief.name.clone();
            let ctx = ctx.clone();
            let nav = nav.clone();
            let brief = brief.clone();
            nav.confirm(
                format!("{}\n{name}", tr("Uninstall this app?")),
                move || {
                    let ctx = ctx.clone();
                    let brief = brief.clone();
                    glib::spawn_future_local(async move {
                        let result = uninstall(&ctx, &brief).await;
                        if result.is_ok() {
                            ctx.refresh_installed().await;
                        }
                    });
                },
            );
        });
        column.append(&remove);
    }
    column
}

fn permission_text(permission: &Permission) -> String {
    match permission {
        Permission::Network => tr("Use the network"),
        Permission::AllFiles => tr("Read and write all files"),
        Permission::Home => tr("Read and write the home folder"),
        Permission::Downloads => tr("Use the Downloads folder"),
        Permission::Devices => tr("Use cameras, microphones, and other devices"),
    }
}

fn heading_of(view: &gtk::Box) -> String {
    let mut child = view.first_child();
    while let Some(widget) = child {
        if let Some(label) = widget.downcast_ref::<gtk::Label>() {
            if label.widget_name() == "bay-heading" {
                return label.label().to_string();
            }
        }
        if let Some(label) = find_named(&widget) {
            return label;
        }
        child = widget.next_sibling();
    }
    String::new()
}

fn find_named(widget: &gtk::Widget) -> Option<String> {
    if let Some(label) = widget.downcast_ref::<gtk::Label>() {
        if label.widget_name() == "bay-heading" {
            return Some(label.label().to_string());
        }
    }
    let mut child = widget.first_child();
    while let Some(next) = child {
        if let Some(found) = find_named(&next) {
            return Some(found);
        }
        child = next.next_sibling();
    }
    None
}

fn update_card(ctx: &Rc<Ctx>, nav: &Nav, brief: &Brief, notes: &str) -> gtk::Box {
    let card = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(6)
        .css_classes(["update-card"])
        .build();
    let top = gtk::Box::builder().spacing(10).build();
    let icon = widgets::icon_picture(48);
    widgets::load_picture(&ctx.http, &icon, &brief.icon);
    top.append(&icon);
    let text = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .hexpand(true)
        .valign(gtk::Align::Center)
        .build();
    text.append(
        &gtk::Label::builder()
            .label(&brief.name)
            .xalign(0.0)
            .css_classes(["bay-app-name"])
            .build(),
    );
    if !brief.version.is_empty() {
        text.append(&widgets::note(&brief.version));
    }
    top.append(&text);
    top.append(&action_button(ctx, brief.clone()));
    card.append(&top);
    if !notes.is_empty() {
        let excerpt = notes.chars().take(180).collect::<String>();
        card.append(&widgets::note(&excerpt));
        let more = gtk::Button::builder()
            .label(tr("more"))
            .css_classes(["linkish", "flat"])
            .halign(gtk::Align::Start)
            .build();
        let nav = nav.clone();
        let id = brief.id.clone();
        more.connect_clicked(move |_| nav.go(Route::App(id.clone())));
        card.append(&more);
    }
    card.set_widget_name(&brief.id.key());
    card
}

async fn fill_update_notes(ctx: &Rc<Ctx>, grid: &gtk::Grid) {
    let mut child = grid.first_child();
    while let Some(widget) = child {
        let name = widget.widget_name().to_string();
        child = widget.next_sibling();
        let Some(id) = name.strip_prefix("flathub:") else {
            continue;
        };
        let Ok(detail) = flathub::detail(&ctx.http, id).await else {
            continue;
        };
        if detail.release_notes.is_empty() {
            continue;
        }
        if let Some(card) = widget.downcast_ref::<gtk::Box>() {
            let excerpt: String = detail.release_notes.chars().take(180).collect();
            card.append(&widgets::note(&excerpt));
        }
    }
}

async fn uninstall(ctx: &Ctx, brief: &Brief) -> Result<(), String> {
    match &brief.id {
        AppId::Flathub(id) | AppId::FlatpakRef(id) => {
            let scope = ctx
                .installed
                .borrow()
                .flatpaks
                .get(id)
                .map(|a| a.scope.clone())
                .unwrap_or_else(|| "system".into());
            install::uninstall_flatpak(id, &scope).await
        }
        AppId::AppImage(id) => {
            let path = ctx
                .installed
                .borrow()
                .images
                .get(id)
                .map(|a| a.path.clone());
            let Some(path) = path else {
                return Err(tr("Not installed."));
            };
            install::uninstall_image(&path)
        }
    }
}
