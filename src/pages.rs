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
use crate::task::{self, BeginError, ImagePlan, Job, JobKind, Phase};
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
        let lists = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(18)
            .build();
        let popular = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let newest = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let rails = gtk::Box::new(gtk::Orientation::Vertical, 0);
        lists.append(&popular);
        lists.append(&newest);
        lists.append(&rails);
        column.append(&lists);
        widgets::flush_pictures(column, &ctx.http);
        let ctx = ctx.clone();
        let nav = nav.clone();
        glib::spawn_future_local(async move {
            let popular_saved = flathub::stored_popular(&ctx.http);
            let newest_saved = flathub::stored_recently_added(&ctx.http);
            let refresh_popular = begin_ranking(
                &popular,
                &ctx,
                &nav,
                &tr("Popular on Flathub"),
                Route::Chart(Chart::Popular),
                popular_saved.clone(),
                flathub::popular_fresh(&ctx.http),
            );
            let refresh_newest = begin_ranking(
                &newest,
                &ctx,
                &nav,
                &tr("New Apps on Flathub"),
                Route::Chart(Chart::New),
                newest_saved.clone(),
                flathub::recently_added_fresh(&ctx.http),
            );
            if refresh_popular {
                finish_ranking(
                    &popular,
                    &ctx,
                    &nav,
                    &tr("Popular on Flathub"),
                    Route::Chart(Chart::Popular),
                    popular_saved,
                    flathub::popular(&ctx.http),
                )
                .await;
            }
            if refresh_newest {
                finish_ranking(
                    &newest,
                    &ctx,
                    &nav,
                    &tr("New Apps on Flathub"),
                    Route::Chart(Chart::New),
                    newest_saved,
                    flathub::recently_added(&ctx.http),
                )
                .await;
            }
            append_rails(&rails, &ctx, &nav, "discover").await;
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
        let lists = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(18)
            .build();
        let popular = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let rails = gtk::Box::new(gtk::Orientation::Vertical, 0);
        lists.append(&popular);
        lists.append(&rails);
        column.append(&lists);
        widgets::flush_pictures(column, &ctx.http);
        let ctx = ctx.clone();
        let nav = nav.clone();
        glib::spawn_future_local(async move {
            let slugs = categories::hub_slugs(&hub_id);
            show_ranking(
                &popular,
                &ctx,
                &nav,
                &tr("Popular on Flathub"),
                Route::Chart(Chart::Hub(hub_id.clone())),
                SavedList {
                    stored: flathub::stored_merged(&ctx.http, slugs),
                    fresh: flathub::merged_fresh(&ctx.http, slugs),
                    fetch: flathub::merged(&ctx.http, slugs),
                },
            )
            .await;
            append_rails(&rails, &ctx, &nav, &hub_id).await;
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
            column.append(&widgets::section(&tr("Curated for App Bay"), None));
            let block = brief_list(&ctx, &nav, &curated, false);
            column.append(&block);
            widgets::flush_pictures(&block, &ctx.http);
        }
        let lists = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(18)
            .build();
        let rails = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let popular = gtk::Box::new(gtk::Orientation::Vertical, 0);
        lists.append(&rails);
        lists.append(&popular);
        column.append(&lists);
        widgets::flush_pictures(column, &ctx.http);
        let ctx = ctx.clone();
        let nav = nav.clone();
        let id = id.clone();
        glib::spawn_future_local(async move {
            let route = Route::Chart(Chart::Category(id.clone()));
            if let Some(slug) = categories::get(&id).and_then(|c| c.flathub) {
                show_ranking(
                    &popular,
                    &ctx,
                    &nav,
                    &tr("Popular on Flathub"),
                    route,
                    SavedList {
                        stored: flathub::stored_category(&ctx.http, slug),
                        fresh: flathub::category_fresh(&ctx.http, slug),
                        fetch: flathub::category(&ctx.http, slug),
                    },
                )
                .await;
            } else {
                let query = categories::title(&id);
                show_ranking(
                    &popular,
                    &ctx,
                    &nav,
                    &tr("Popular on Flathub"),
                    route,
                    SavedList {
                        stored: flathub::stored_search(&ctx.http, &query),
                        fresh: flathub::search_fresh(&ctx.http, &query),
                        fetch: flathub::search(&ctx.http, &query),
                    },
                )
                .await;
            }
            append_rails(&rails, &ctx, &nav, &id).await;
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
            match &chart {
                Chart::Popular => {
                    show_chart(
                        &slot,
                        &ctx,
                        &nav,
                        flathub::stored_popular(&ctx.http),
                        flathub::popular_fresh(&ctx.http),
                        flathub::popular(&ctx.http),
                    )
                    .await;
                }
                Chart::New => {
                    show_chart(
                        &slot,
                        &ctx,
                        &nav,
                        flathub::stored_recently_added(&ctx.http),
                        flathub::recently_added_fresh(&ctx.http),
                        flathub::recently_added(&ctx.http),
                    )
                    .await;
                }
                Chart::Hub(id) => {
                    let slugs = categories::hub_slugs(id);
                    show_chart(
                        &slot,
                        &ctx,
                        &nav,
                        flathub::stored_merged(&ctx.http, slugs),
                        flathub::merged_fresh(&ctx.http, slugs),
                        flathub::merged(&ctx.http, slugs),
                    )
                    .await;
                }
                Chart::Category(id) => match categories::get(id).and_then(|c| c.flathub) {
                    Some(slug) => {
                        show_chart(
                            &slot,
                            &ctx,
                            &nav,
                            flathub::stored_category(&ctx.http, slug),
                            flathub::category_fresh(&ctx.http, slug),
                            flathub::category(&ctx.http, slug),
                        )
                        .await;
                    }
                    None => {
                        let query = categories::title(id);
                        show_chart(
                            &slot,
                            &ctx,
                            &nav,
                            flathub::stored_search(&ctx.http, &query),
                            flathub::search_fresh(&ctx.http, &query),
                            flathub::search(&ctx.http, &query),
                        )
                        .await;
                    }
                },
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
            column.append(&widgets::section(&tr("Curated for App Bay"), None));
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
        let local_empty = local.is_empty();
        glib::spawn_future_local(async move {
            let stored = flathub::stored_search(&ctx.http, &query);
            let fresh = flathub::search_fresh(&ctx.http, &query);
            if let Some(hits) = &stored {
                paint_search(&slot, &ctx, &nav, hits, local_empty);
            } else {
                slot.append(&widgets::loading(&tr("Loading…")));
            }
            if fresh && stored.is_some() {
                return;
            }
            match flathub::search(&ctx.http, &query).await {
                Ok(hits) if stored.as_ref().is_some_and(|saved| same_hits(saved, &hits)) => {}
                Ok(hits) => paint_search(&slot, &ctx, &nav, &hits, local_empty),
                Err(err) if stored.is_some() => {
                    let _ = err;
                }
                Err(err) => {
                    clear_box(&slot);
                    slot.append(&widgets::note(&err));
                }
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
        slot.append(&widgets::loading(&tr("Loading…")));
        let nav = nav.clone();
        glib::spawn_future_local(async move {
            let built = match &id {
                AppId::Flathub(fid) => match flathub::detail(&ctx.http, fid).await {
                    Ok(detail) => Ok(detail_view(&ctx, &nav, &detail)),
                    Err(err) => Err(err),
                },
                AppId::AppImage(iid) | AppId::FlatpakRef(iid) => {
                    let Some(app) = ctx.catalog.borrow().curated(iid).cloned() else {
                        clear_box(&slot);
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
            clear_box(&slot);
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
        let ctx = ctx_button.clone();
        let nav = nav_button.clone();
        match ctx.try_begin(ALL_JOB, JobKind::Flatpak) {
            Ok(()) => {}
            Err(BeginError::Duplicate) => return,
            Err(BeginError::FlatpakBusy) => {
                ctx.fail_job(ALL_JOB, tr("Another install or update is already running."));
                return;
            }
        }
        button.set_label(&tr("Updating…"));
        glib::spawn_future_local(async move {
            let err = update_everything(&ctx).await.err();
            if let Some(err) = err {
                ctx.fail_job(ALL_JOB, err);
            } else {
                ctx.clear_job(ALL_JOB);
            }
            nav.go(Route::Updates);
        });
    });
    let ctx_all = ctx.clone();
    let all_button = update_all.downgrade();
    let all_watch = ctx.watch_jobs(move || {
        let Some(button) = all_button.upgrade() else {
            return;
        };
        match ctx_all.job(ALL_JOB) {
            Some(Job::Working {
                phase, fraction, ..
            }) => {
                button.set_label(&widgets::working_text(phase, fraction));
            }
            Some(Job::Failed { message }) => {
                button.set_label(&tr("Try Again"));
                button.set_tooltip_text(Some(&message));
            }
            None => {
                button.set_label(&tr("Update All"));
                button.set_tooltip_text(None);
            }
        }
    });
    let ctx_unwatch = ctx.clone();
    update_all.connect_destroy(move |_| ctx_unwatch.unwatch(all_watch));
    widgets::page(
        &tr("Updates"),
        Some(update_all.upcast_ref()),
        move |column| {
            let slot = gtk::Box::builder()
                .orientation(gtk::Orientation::Vertical)
                .spacing(12)
                .build();
            column.append(&slot);
            slot.append(&widgets::loading(&tr("Checking for updates…")));
            glib::spawn_future_local(async move {
                if let Err(err) = ctx.refresh_updates().await {
                    clear_box(&slot);
                    slot.append(&widgets::note(&err));
                    return;
                }
                clear_box(&slot);
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
                        update: String::new(),
                    };
                    cards.push(update_card(&ctx, &nav, &brief, "").upcast());
                }
                let listings = ctx.catalog_listings();
                for (id, app) in &installed.images {
                    let Some((version, download, update)) = listings.get(id) else {
                        continue;
                    };
                    if !matches!(
                        task::image_plan(&app.version, version, download, update, &app.update_info),
                        ImagePlan::Ready(_)
                    ) {
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
        let known = ctx.installed.borrow().clone();
        if known.flatpaks.is_empty() && known.images.is_empty() {
            slot.append(&widgets::loading(&tr("Loading…")));
        } else {
            paint_installed(&slot, &ctx, &nav, &known);
        }
        let before = known.revision();
        let had_apps = !known.flatpaks.is_empty() || !known.images.is_empty();
        glib::spawn_future_local(async move {
            ctx.refresh_installed().await;
            let installed = ctx.installed.borrow().clone();
            if installed.revision() == before && had_apps {
                return;
            }
            clear_box(&slot);
            if installed.flatpaks.is_empty() && installed.images.is_empty() {
                slot.append(&widgets::note(&tr("Nothing is installed yet.")));
                return;
            }
            paint_installed(&slot, &ctx, &nav, &installed);
        });
    })
}

fn append_ranking(
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

/// Paint a saved list, or a spinner when there is no copy yet.
/// Returns whether a refresh should still run.
fn begin_ranking(
    slot: &gtk::Box,
    ctx: &Rc<Ctx>,
    nav: &Nav,
    title: &str,
    more: Route,
    stored: Option<Vec<Hit>>,
    fresh: bool,
) -> bool {
    if let Some(hits) = stored {
        append_ranking(slot, ctx, nav, title, Ok(hits), more);
        !fresh
    } else {
        slot.append(&widgets::loading(&tr("Loading…")));
        true
    }
}

/// Replace the section when a newer list arrives. A failed refresh leaves
/// the list `begin_ranking` already painted.
async fn finish_ranking(
    slot: &gtk::Box,
    ctx: &Rc<Ctx>,
    nav: &Nav,
    title: &str,
    more: Route,
    stored: Option<Vec<Hit>>,
    fetch: impl std::future::Future<Output = Result<Vec<Hit>, String>>,
) {
    match fetch.await {
        Ok(hits) if stored.as_ref().is_some_and(|saved| same_hits(saved, &hits)) => {}
        Ok(hits) => {
            clear_box(slot);
            append_ranking(slot, ctx, nav, title, Ok(hits), more);
        }
        Err(err) => {
            if stored.is_none() {
                clear_box(slot);
                slot.append(&widgets::note(&err));
            }
        }
    }
}

struct SavedList<F> {
    stored: Option<Vec<Hit>>,
    fresh: bool,
    fetch: F,
}

async fn show_ranking(
    slot: &gtk::Box,
    ctx: &Rc<Ctx>,
    nav: &Nav,
    title: &str,
    more: Route,
    list: SavedList<impl std::future::Future<Output = Result<Vec<Hit>, String>>>,
) {
    if begin_ranking(
        slot,
        ctx,
        nav,
        title,
        more.clone(),
        list.stored.clone(),
        list.fresh,
    ) {
        finish_ranking(slot, ctx, nav, title, more, list.stored, list.fetch).await;
    }
}

async fn show_chart(
    slot: &gtk::Box,
    ctx: &Rc<Ctx>,
    nav: &Nav,
    stored: Option<Vec<Hit>>,
    fresh: bool,
    fetch: impl std::future::Future<Output = Result<Vec<Hit>, String>>,
) {
    if let Some(hits) = &stored {
        paint_chart(slot, ctx, nav, hits);
    } else {
        slot.append(&widgets::loading(&tr("Loading…")));
    }
    if fresh && stored.is_some() {
        return;
    }
    match fetch.await {
        Ok(hits) if stored.as_ref().is_some_and(|saved| same_hits(saved, &hits)) => {}
        Ok(hits) => paint_chart(slot, ctx, nav, &hits),
        Err(err) => {
            if stored.is_none() {
                clear_box(slot);
                slot.append(&widgets::note(&err));
            }
        }
    }
}

fn paint_chart(slot: &gtk::Box, ctx: &Rc<Ctx>, nav: &Nav, hits: &[Hit]) {
    clear_box(slot);
    if hits.is_empty() {
        return;
    }
    let block = hit_list(ctx, nav, hits, 60, true);
    slot.append(&block);
    widgets::flush_pictures(&block, &ctx.http);
}

fn paint_search(slot: &gtk::Box, ctx: &Rc<Ctx>, nav: &Nav, hits: &[Hit], local_empty: bool) {
    clear_box(slot);
    if hits.is_empty() {
        if local_empty {
            slot.append(&widgets::note(&tr("No matching apps.")));
        }
        return;
    }
    slot.append(&widgets::section(&tr("Flathub"), None));
    let block = hit_list(ctx, nav, hits, 40, false);
    slot.append(&block);
    widgets::flush_pictures(&block, &ctx.http);
}

fn paint_installed(
    slot: &gtk::Box,
    ctx: &Rc<Ctx>,
    nav: &Nav,
    installed: &crate::installed::Installed,
) {
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
            update: String::new(),
        });
    }
    for (id, app) in &installed.images {
        let mut brief = ctx
            .catalog
            .borrow()
            .curated(id)
            .cloned()
            .map(|a| brief_from_curated(ctx, &a))
            .unwrap_or(Brief {
                id: AppId::AppImage(id.clone()),
                name: app.name.clone(),
                summary: String::new(),
                icon: String::new(),
                developer: String::new(),
                version: app.version.clone(),
                download: String::new(),
                update: String::new(),
            });
        brief.summary = app.version.clone();
        briefs.push(brief);
    }
    briefs.sort_by_key(|a| a.name.to_lowercase());
    let block = brief_list(ctx, nav, &briefs, false);
    slot.append(&block);
    widgets::flush_pictures(&block, &ctx.http);
}

fn same_hits(saved: &[Hit], fresh: &[Hit]) -> bool {
    saved.len() == fresh.len()
        && saved
            .iter()
            .zip(fresh)
            .all(|(a, b)| a.app_id == b.app_id && a.name == b.name)
}

fn clear_box(slot: &gtk::Box) {
    while let Some(child) = slot.first_child() {
        slot.remove(&child);
    }
}

async fn append_rails(slot: &gtk::Box, ctx: &Rc<Ctx>, nav: &Nav, surface: &str) {
    let rails: Vec<Rail> = model::rails_for(&ctx.catalog.borrow().rails, surface)
        .into_iter()
        .cloned()
        .collect();
    let waiting = rails_need_network(ctx, &rails);
    if waiting {
        slot.append(&widgets::loading(&tr("Loading…")));
    }
    let mut blocks: Vec<gtk::Widget> = Vec::new();
    for rail in rails {
        let briefs = resolve_ids(ctx, &rail.apps).await;
        if briefs.is_empty() {
            continue;
        }
        blocks.push(widgets::section(&rail.title, None).upcast());
        let block = brief_list(ctx, nav, &briefs, false);
        widgets::flush_pictures(&block, &ctx.http);
        blocks.push(block.upcast());
    }
    if waiting {
        clear_box(slot);
    }
    for block in blocks {
        slot.append(&block);
    }
}

fn rails_need_network(ctx: &Ctx, rails: &[Rail]) -> bool {
    rails.iter().any(|rail| {
        rail.apps.iter().any(|raw| match AppId::parse(raw) {
            AppId::Flathub(id) => !flathub::detail_fresh(&ctx.http, &id),
            AppId::AppImage(_) | AppId::FlatpakRef(_) => false,
        })
    })
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
    click.connect_released(move |gesture, _, x, y| {
        if let Some(row) = gesture.widget() {
            if let Some(target) = row.pick(x, y, gtk::PickFlags::DEFAULT) {
                let mut widget = Some(target);
                while let Some(next) = widget {
                    if next.downcast_ref::<gtk::Button>().is_some() {
                        return;
                    }
                    widget = next.parent();
                }
            }
        }
        nav.go(Route::App(id.clone()));
    });
    row.add_controller(click);
}

const ALL_JOB: &str = "*flatpak-all*";

thread_local! {
    static CONTROLS: std::cell::RefCell<Vec<(u64, Rc<widgets::ActionControl>)>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

fn action_button(ctx: &Rc<Ctx>, brief: Brief) -> gtk::Box {
    let offer = ctx.offer(&brief.id, &brief.version, &brief.download, &brief.update);
    let control = Rc::new(widgets::ActionControl::new(&widgets::offer_label(offer)));
    let ctx_watch = ctx.clone();
    let brief_watch = brief.clone();
    let weak = Rc::downgrade(&control);
    let watch = ctx.watch_jobs(move || {
        let Some(control) = weak.upgrade() else {
            return;
        };
        if let Some(job) = ctx_watch.job(&brief_watch.id.key()) {
            control.show_job(&job);
        } else {
            control.show_offer(ctx_watch.offer(
                &brief_watch.id,
                &brief_watch.version,
                &brief_watch.download,
                &brief_watch.update,
            ));
        }
    });
    let kept = next_control_id();
    CONTROLS.with(|slot| slot.borrow_mut().push((kept, control.clone())));
    let ctx_drop = ctx.clone();
    let saw_parent = std::rc::Rc::new(std::cell::Cell::new(false));
    control
        .root
        .connect_notify_local(Some("parent"), move |widget, _| {
            if widget.parent().is_some() {
                saw_parent.set(true);
            } else if saw_parent.get() {
                ctx_drop.unwatch(watch);
                CONTROLS.with(|slot| slot.borrow_mut().retain(|(id, _)| *id != kept));
            }
        });
    let ctx_click = ctx.clone();
    let brief_click = brief;
    control.connect_clicked(move |_| on_action(&ctx_click, &brief_click));
    control.root.clone()
}

fn next_control_id() -> u64 {
    thread_local! {
        static NEXT: std::cell::Cell<u64> = const { std::cell::Cell::new(1) };
    }
    NEXT.with(|next| {
        let id = next.get();
        next.set(id.wrapping_add(1));
        id
    })
}

fn on_action(ctx: &Rc<Ctx>, brief: &Brief) {
    let key = brief.id.key();
    if matches!(ctx.job(&key), Some(Job::Working { .. })) {
        return;
    }
    let offer = ctx.offer(&brief.id, &brief.version, &brief.download, &brief.update);
    if offer == Offer::Open {
        ctx.clear_job(&key);
        let path = image_path(ctx, &brief.id);
        if let Err(err) = install::open_app(&brief.id, path.as_deref()) {
            ctx.fail_job(&key, err);
        }
        return;
    }
    let kind = if matches!(brief.id, AppId::AppImage(_)) {
        JobKind::Image
    } else {
        JobKind::Flatpak
    };
    match ctx.try_begin(&key, kind) {
        Ok(()) => {}
        Err(BeginError::Duplicate) => return,
        Err(BeginError::FlatpakBusy) => {
            ctx.fail_job(&key, tr("Another install or update is already running."));
            return;
        }
    }
    let ctx = ctx.clone();
    let brief = brief.clone();
    glib::spawn_future_local(async move {
        let key = brief.id.key();
        let ctx_report = ctx.clone();
        let key_report = key.clone();
        let mut report = move |phase, fraction| ctx_report.report(&key_report, phase, fraction);
        let result = perform(&ctx, &brief, &mut report).await;
        match result {
            Ok(Done::Flatpak(id)) => {
                ctx.report(&key, Phase::Finishing, None);
                ctx.settle_flatpak(id.as_deref()).await;
                let offer = ctx.offer(&brief.id, &brief.version, &brief.download, &brief.update);
                if offer == Offer::Get {
                    ctx.fail_job(&key, tr("The install finished without the app appearing."));
                } else {
                    ctx.clear_job(&key);
                }
            }
            Ok(Done::Image) => {
                ctx.report(&key, Phase::Finishing, None);
                ctx.refresh_installed().await;
                let offer = ctx.offer(&brief.id, &brief.version, &brief.download, &brief.update);
                if matches!(brief.id, AppId::AppImage(_)) && offer == Offer::Get {
                    ctx.fail_job(&key, tr("The install finished without the app appearing."));
                } else {
                    ctx.clear_job(&key);
                }
            }
            Err(err) => ctx.fail_job(&key, err),
        }
    });
}

enum Done {
    Flatpak(Option<String>),
    Image,
}

async fn perform(
    ctx: &Ctx,
    brief: &Brief,
    progress: &mut dyn FnMut(Phase, Option<f64>),
) -> Result<Done, String> {
    match &brief.id {
        AppId::Flathub(id) if ctx.installed.borrow().flatpaks.contains_key(id) => {
            let scope = flatpak_scope(ctx, id).await;
            install::update_flatpak(id, &scope, progress).await?;
            Ok(Done::Flatpak(Some(id.clone())))
        }
        AppId::Flathub(id) => {
            let scope = flatpak_scope(ctx, id).await;
            install::install_flathub(id, &brief.name, &scope, progress).await?;
            Ok(Done::Flatpak(Some(id.clone())))
        }
        AppId::FlatpakRef(id) if ctx.installed.borrow().flatpaks.contains_key(id) => {
            let scope = flatpak_scope(ctx, id).await;
            install::update_flatpak(id, &scope, progress).await?;
            Ok(Done::Flatpak(Some(id.clone())))
        }
        AppId::FlatpakRef(id) => {
            if brief.download.is_empty() {
                return Err(tr("No download is set for this app."));
            }
            let scope = flatpak_scope(ctx, id).await;
            let path = crate::http::downloads_dir()
                .join(install::download_name(&brief.id, &brief.download));
            install::download_to(&ctx.http, &brief.download, &path, progress).await?;
            let installed = install::install_flatpak_file(&path, &scope, progress).await;
            let _ = std::fs::remove_file(&path);
            installed?;
            Ok(Done::Flatpak(Some(id.clone())))
        }
        AppId::AppImage(id) => {
            if ctx.installed.borrow().images.contains_key(id) {
                update_image(ctx, brief, id, progress).await?;
            } else {
                if brief.download.is_empty() {
                    return Err(tr("No download is set for this app."));
                }
                install::install_appimage(
                    &ctx.http,
                    &brief.download,
                    &brief.name,
                    id,
                    &brief.version,
                    progress,
                )
                .await?;
            }
            Ok(Done::Image)
        }
    }
}

async fn update_image(
    ctx: &Ctx,
    brief: &Brief,
    id: &str,
    progress: &mut dyn FnMut(Phase, Option<f64>),
) -> Result<(), String> {
    let plan =
        ctx.installed
            .borrow()
            .image_plan_for(id, &brief.version, &brief.download, &brief.update);
    let ImagePlan::Ready(provider) = plan else {
        return Err(task::unsupported_image_message().into());
    };
    let app = ctx
        .installed
        .borrow()
        .images
        .get(id)
        .cloned()
        .ok_or_else(|| task::unsupported_image_message().to_owned())?;
    let source = if !app.source.is_empty() {
        app.source.clone()
    } else {
        app.path
            .read_dir()
            .ok()
            .and_then(|read| {
                read.flatten().find_map(|entry| {
                    let name = entry.file_name().to_string_lossy().into_owned();
                    name.to_ascii_lowercase()
                        .ends_with(".appimage")
                        .then_some(name)
                })
            })
            .ok_or_else(|| {
                "The installed AppImage is missing, so it was left unchanged.".to_owned()
            })?
    };
    install::update_appimage(&app.path, &source, &provider, &ctx.http, progress).await
}

async fn flatpak_scope(ctx: &Ctx, id: &str) -> String {
    if let Some(scope) = ctx
        .installed
        .borrow()
        .flatpaks
        .get(id)
        .map(|app| app.scope.clone())
        .filter(|scope| !scope.is_empty())
    {
        return scope;
    }
    install::default_install_scope().await
}

async fn update_everything(ctx: &Rc<Ctx>) -> Result<(), String> {
    let installed = ctx.installed.borrow().clone();
    let mut scopes: Vec<String> = installed
        .updates
        .keys()
        .map(|id| {
            installed
                .flatpaks
                .get(id)
                .map(|app| app.scope.clone())
                .filter(|scope| !scope.is_empty())
                .unwrap_or_else(|| "system".into())
        })
        .collect();
    scopes.sort();
    scopes.dedup();
    if scopes.is_empty() {
        scopes.push("system".into());
        scopes.push("user".into());
    }
    let ids: Vec<String> = installed.updates.keys().cloned().collect();
    for id in &ids {
        ctx.follow_job(&AppId::Flathub(id.clone()).key(), JobKind::Flatpak);
    }
    let ctx_report = ctx.clone();
    let ids_report = ids.clone();
    let mut report = move |phase, fraction| {
        ctx_report.report(ALL_JOB, phase, fraction);
        for id in &ids_report {
            ctx_report.report(&AppId::Flathub(id.clone()).key(), phase, fraction);
        }
    };
    for scope in &scopes {
        if let Err(err) = install::update_installation(scope, &mut report).await {
            for id in &ids {
                ctx.fail_job(&AppId::Flathub(id.clone()).key(), err.clone());
            }
            return Err(err);
        }
    }
    ctx.report(ALL_JOB, Phase::Finishing, None);
    ctx.settle_flatpak(None).await;
    for id in &ids {
        ctx.clear_job(&AppId::Flathub(id.clone()).key());
    }
    let listings = ctx.catalog_listings();
    let image_ids: Vec<String> = ctx.installed.borrow().images.keys().cloned().collect();
    let mut images = Vec::new();
    for id in image_ids {
        let Some(curated) = ctx.catalog.borrow().curated(&id).cloned() else {
            continue;
        };
        let Some(app) = ctx.installed.borrow().images.get(&id).cloned() else {
            continue;
        };
        let Some((version, download, update)) = listings.get(&id) else {
            continue;
        };
        if !matches!(
            task::image_plan(&app.version, version, download, update, &app.update_info),
            ImagePlan::Ready(_)
        ) {
            continue;
        }
        images.push((id, brief_from_curated(ctx, &curated)));
    }
    for (id, brief) in images {
        let key = brief.id.key();
        if ctx.try_begin(&key, JobKind::Image).is_err() {
            continue;
        }
        let mut report = {
            let ctx = ctx.clone();
            let key = key.clone();
            move |phase, fraction| ctx.report(&key, phase, fraction)
        };
        if let Err(err) = update_image(ctx, &brief, &id, &mut report).await {
            ctx.fail_job(&key, err);
            continue;
        }
        ctx.refresh_installed().await;
        ctx.clear_job(&key);
    }
    Ok(())
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
        update: app.update.clone(),
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
    if ctx.offer(&brief.id, &brief.version, &brief.download, &brief.update) != Offer::Get {
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
