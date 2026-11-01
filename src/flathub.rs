//! Flathub's v2 API. Ranked lists keep the order Flathub sends.

use serde_json::Value;

use crate::http::Http;
use crate::model::{AppId, Brief};
use crate::text::{self, Permission};

const BASE: &str = "https://flathub.org/api/v2";
// Popular, New, and category lists. Monthly install counts move slowly,
// and a new app is worth picking up a couple of times a day.
const RANK_TTL: u64 = 12 * 60 * 60;
// A query already run. A query App Bay has never seen still goes to Flathub.
const SEARCH_TTL: u64 = 12 * 60 * 60;
// Descriptions and screenshots change when an app ships an update.
const DETAIL_TTL: u64 = 24 * 60 * 60;

#[derive(Clone, Debug)]
pub struct Hit {
    pub app_id: String,
    pub name: String,
    pub summary: String,
    pub icon: String,
    pub developer: String,
    pub installs: u64,
}

impl Hit {
    pub fn brief(&self) -> Brief {
        Brief {
            id: AppId::Flathub(self.app_id.clone()),
            name: self.name.clone(),
            summary: self.summary.clone(),
            icon: self.icon.clone(),
            developer: self.developer.clone(),
            version: String::new(),
            download: String::new(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Detail {
    pub brief: Brief,
    pub description: String,
    pub homepage: String,
    pub screenshots: Vec<String>,
    pub release_version: String,
    pub release_date: String,
    pub release_notes: String,
    pub download_size: Option<u64>,
    pub permissions: Vec<Permission>,
}

pub fn parse_hits(value: &Value) -> Vec<Hit> {
    let Some(hits) = value.get("hits").and_then(|v| v.as_array()) else {
        return Vec::new();
    };
    hits.iter().filter_map(parse_hit).collect()
}

fn parse_hit(value: &Value) -> Option<Hit> {
    let app_id = value.get("app_id").and_then(|v| v.as_str())?.to_owned();
    if app_id.is_empty() {
        return None;
    }
    Some(Hit {
        name: text_field(value, "name").unwrap_or_else(|| app_id.clone()),
        summary: text_field(value, "summary").unwrap_or_default(),
        icon: text_field(value, "icon").unwrap_or_default(),
        developer: text_field(value, "developer_name").unwrap_or_default(),
        installs: value
            .get("installs_last_month")
            .and_then(|v| v.as_u64())
            .unwrap_or(0),
        app_id,
    })
}

fn text_field(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

pub fn stored_popular(http: &Http) -> Option<Vec<Hit>> {
    stored_ranking(http, &popular_url())
}

pub fn popular_fresh(http: &Http) -> bool {
    http.fresh("GET", &popular_url(), None, RANK_TTL)
}

pub async fn popular(http: &Http) -> Result<Vec<Hit>, String> {
    ranking(http, &popular_url()).await
}

pub fn stored_recently_added(http: &Http) -> Option<Vec<Hit>> {
    stored_ranking(http, &recent_url())
}

pub fn recently_added_fresh(http: &Http) -> bool {
    http.fresh("GET", &recent_url(), None, RANK_TTL)
}

pub async fn recently_added(http: &Http) -> Result<Vec<Hit>, String> {
    ranking(http, &recent_url()).await
}

pub fn stored_category(http: &Http, slug: &str) -> Option<Vec<Hit>> {
    stored_ranking(http, &category_url(slug))
}

pub fn category_fresh(http: &Http, slug: &str) -> bool {
    http.fresh("GET", &category_url(slug), None, RANK_TTL)
}

pub async fn category(http: &Http, slug: &str) -> Result<Vec<Hit>, String> {
    ranking(http, &category_url(slug)).await
}

fn popular_url() -> String {
    format!("{BASE}/collection/popular")
}

fn recent_url() -> String {
    format!("{BASE}/collection/recently-added")
}

fn category_url(slug: &str) -> String {
    format!("{BASE}/collection/category/{slug}")
}

/// Merge Flathub's per-category rankings, keeping the higher install count
/// when an app is in more than one.
/// Last saved rankings for every slug, merged. Missing one means no copy yet.
pub fn stored_merged(http: &Http, slugs: &[&str]) -> Option<Vec<Hit>> {
    let mut groups = Vec::new();
    for slug in slugs {
        groups.push(stored_category(http, slug)?);
    }
    Some(merge(groups))
}

pub fn merged_fresh(http: &Http, slugs: &[&str]) -> bool {
    slugs.iter().all(|slug| category_fresh(http, slug))
}

pub async fn merged(http: &Http, slugs: &[&str]) -> Result<Vec<Hit>, String> {
    let mut groups = Vec::new();
    for slug in slugs {
        groups.push(category(http, slug).await?);
    }
    Ok(merge(groups))
}

fn merge(groups: Vec<Vec<Hit>>) -> Vec<Hit> {
    let mut best: Vec<Hit> = Vec::new();
    for group in groups {
        for hit in group {
            if let Some(existing) = best.iter_mut().find(|h| h.app_id == hit.app_id) {
                if hit.installs > existing.installs {
                    *existing = hit;
                }
            } else {
                best.push(hit);
            }
        }
    }
    best.sort_by(|a, b| b.installs.cmp(&a.installs).then(a.name.cmp(&b.name)));
    best
}

pub fn stored_search(http: &Http, query: &str) -> Option<Vec<Hit>> {
    stored_hits(http, "POST", &search_url(), Some(&search_body(query)))
}

pub fn search_fresh(http: &Http, query: &str) -> bool {
    http.fresh("POST", &search_url(), Some(&search_body(query)), SEARCH_TTL)
}

pub async fn search(http: &Http, query: &str) -> Result<Vec<Hit>, String> {
    let body = search_body(query);
    let bytes = http.post(&search_url(), &body, SEARCH_TTL).await?;
    let value: Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    Ok(parse_hits(&value))
}

fn search_url() -> String {
    format!("{BASE}/search")
}

fn search_body(query: &str) -> String {
    serde_json::json!({ "query": query }).to_string()
}

pub fn detail_fresh(http: &Http, app_id: &str) -> bool {
    http.fresh(
        "GET",
        &format!("{BASE}/appstream/{app_id}"),
        None,
        DETAIL_TTL,
    ) && http.fresh("GET", &format!("{BASE}/summary/{app_id}"), None, DETAIL_TTL)
}

fn stored_ranking(http: &Http, url: &str) -> Option<Vec<Hit>> {
    stored_hits(http, "GET", url, None)
}

fn stored_hits(http: &Http, method: &str, url: &str, body: Option<&str>) -> Option<Vec<Hit>> {
    let bytes = http.stored(method, url, body)?;
    let value: Value = serde_json::from_slice(&bytes).ok()?;
    value.get("hits")?;
    Some(parse_hits(&value))
}

async fn ranking(http: &Http, url: &str) -> Result<Vec<Hit>, String> {
    let bytes = http.get(url, RANK_TTL).await?;
    let value: Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    Ok(parse_hits(&value))
}

pub async fn detail(http: &Http, app_id: &str) -> Result<Detail, String> {
    let app_bytes = http
        .get(&format!("{BASE}/appstream/{app_id}"), DETAIL_TTL)
        .await?;
    let app: Value = serde_json::from_slice(&app_bytes).map_err(|e| e.to_string())?;
    let summary = http
        .get(&format!("{BASE}/summary/{app_id}"), DETAIL_TTL)
        .await
        .ok();
    let summary: Option<Value> = summary.and_then(|b| serde_json::from_slice(&b).ok());
    Ok(parse_detail(app_id, &app, summary.as_ref()))
}

fn parse_detail(app_id: &str, app: &Value, summary: Option<&Value>) -> Detail {
    let name = text_field(app, "name").unwrap_or_else(|| app_id.to_owned());
    let release = app
        .get("releases")
        .and_then(|v| v.as_array())
        .and_then(|a| a.first());
    let (release_version, release_date, release_notes) = release
        .map(|rel| {
            (
                text_field(rel, "version").unwrap_or_default(),
                rel.get("timestamp")
                    .and_then(|v| v.as_str())
                    .and_then(|s| s.parse::<i64>().ok())
                    .map(format_date)
                    .unwrap_or_default(),
                rel.get("description")
                    .and_then(|v| v.as_str())
                    .map(text::html_to_text)
                    .unwrap_or_default(),
            )
        })
        .unwrap_or_default();
    Detail {
        brief: Brief {
            id: AppId::Flathub(app_id.to_owned()),
            name,
            summary: text_field(app, "summary").unwrap_or_default(),
            icon: text_field(app, "icon").unwrap_or_default(),
            developer: text_field(app, "developer_name").unwrap_or_default(),
            version: release_version.clone(),
            download: String::new(),
        },
        description: app
            .get("description")
            .and_then(|v| v.as_str())
            .map(text::html_to_text)
            .unwrap_or_default(),
        homepage: app
            .get("urls")
            .and_then(|u| u.get("homepage"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned(),
        screenshots: screenshots(app),
        release_version,
        release_date,
        release_notes,
        download_size: summary
            .and_then(|s| s.get("download_size"))
            .and_then(|v| v.as_u64()),
        permissions: summary.map(text::permissions).unwrap_or_default(),
    }
}

fn screenshots(app: &Value) -> Vec<String> {
    let Some(list) = app.get("screenshots").and_then(|v| v.as_array()) else {
        return Vec::new();
    };
    list.iter().filter_map(screenshot_src).take(8).collect()
}

fn screenshot_src(shot: &Value) -> Option<String> {
    let sizes = shot.get("sizes").and_then(|v| v.as_array())?;
    let mut best: Option<(i64, String)> = None;
    for size in sizes {
        let src = size.get("src").and_then(|v| v.as_str())?;
        let width = size
            .get("width")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or(0);
        let distance = (width - 752).abs();
        if best.as_ref().is_none_or(|b| distance < b.0) {
            best = Some((distance, src.to_owned()));
        }
    }
    best.map(|(_, src)| src)
}

fn format_date(unix: i64) -> String {
    gtk::glib::DateTime::from_unix_local(unix)
        .ok()
        .and_then(|d| d.format("%b %-d, %Y").ok())
        .map(|s| s.to_string())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_flathub_order() {
        let value = serde_json::json!({
            "hits": [
                {"app_id": "org.a.A", "name": "A", "summary": "one", "installs_last_month": 10},
                {"app_id": "org.b.B", "name": "B", "summary": "two", "installs_last_month": 3}
            ]
        });
        let hits = parse_hits(&value);
        assert_eq!(hits[0].app_id, "org.a.A");
        assert_eq!(hits[1].installs, 3);
    }
}
