//! libsoup on the GTK main loop, with a small disk cache.

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use std::path::Path;

use gtk::gio::prelude::InputStreamExtManual;
use gtk::glib;
use soup::prelude::*;

#[derive(Clone)]
pub struct Http {
    session: soup::Session,
    dir: PathBuf,
}

impl Http {
    pub fn new() -> Self {
        let session = soup::Session::new();
        session.set_user_agent("elavo-app-bay");
        session.set_timeout(30);
        let dir = cache_dir().join("http");
        let _ = std::fs::create_dir_all(&dir);
        Self { session, dir }
    }

    pub fn get(
        &self,
        url: &str,
        ttl_secs: u64,
    ) -> impl std::future::Future<Output = Result<Vec<u8>, String>> + 'static {
        self.request("GET", url, None, ttl_secs)
    }

    pub fn post(
        &self,
        url: &str,
        body: &str,
        ttl_secs: u64,
    ) -> impl std::future::Future<Output = Result<Vec<u8>, String>> + 'static {
        self.request("POST", url, Some(body.to_owned()), ttl_secs)
    }

    /// Always ask the network, then save the body. A failure returns the last
    /// saved body so a catalog page can stay up when elavo.io does not answer.
    pub fn get_latest(
        &self,
        url: &str,
    ) -> impl std::future::Future<Output = Result<Vec<u8>, String>> + 'static {
        let session = self.session.clone();
        let path = self.dir.join(cache_key("GET", url, None));
        let url = url.to_owned();
        async move {
            match fetch(&session, "GET", &url, None).await {
                Ok(bytes) => {
                    write_cache(&path, &bytes);
                    Ok(bytes)
                }
                Err(err) => read_stored(&path).ok_or(err),
            }
        }
    }

    fn request(
        &self,
        method: &str,
        url: &str,
        body: Option<String>,
        ttl_secs: u64,
    ) -> impl std::future::Future<Output = Result<Vec<u8>, String>> + 'static {
        let session = self.session.clone();
        let path = self.dir.join(cache_key(method, url, body.as_deref()));
        let method = method.to_owned();
        let url = url.to_owned();
        async move {
            if ttl_secs > 0 {
                if let Some(hit) = read_cache(&path, ttl_secs) {
                    return Ok(hit);
                }
            }
            let fetched = fetch(&session, &method, &url, body.as_deref()).await;
            match fetched {
                Ok(vec) => {
                    if ttl_secs > 0 {
                        write_cache(&path, &vec);
                    }
                    Ok(vec)
                }
                Err(err) => {
                    // A failed refresh keeps the last copy. The page can
                    // stay on what it already showed.
                    if ttl_secs > 0 {
                        if let Some(stale) = read_stored(&path) {
                            return Ok(stale);
                        }
                    }
                    Err(err)
                }
            }
        }
    }

    /// The last body saved for this request, including one past its usual age.
    pub fn stored(&self, method: &str, url: &str, body: Option<&str>) -> Option<Vec<u8>> {
        read_stored(&self.dir.join(cache_key(method, url, body)))
    }

    /// Saves `url` to `dest`. `progress` gets bytes received and the total
    /// when the server sent one. A failed download removes the partial file.
    pub async fn download_file(
        &self,
        url: &str,
        dest: &Path,
        mut progress: impl FnMut(u64, Option<u64>),
    ) -> Result<(), String> {
        let message = soup::Message::new("GET", url).map_err(|e| e.to_string())?;
        let stream = self
            .session
            .send_future(&message, glib::Priority::DEFAULT)
            .await
            .map_err(|e| e.to_string())?;
        let code = message.status_code();
        if !(200..300).contains(&code) {
            return Err(format!("HTTP {code} for {url}"));
        }
        let total = message
            .response_headers()
            .map(|headers| headers.content_length())
            .filter(|n| *n > 0)
            .map(|n| n as u64);
        if let Some(dir) = dest.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let partial = partial_path(dest);
        let mut file = std::fs::File::create(&partial).map_err(|e| e.to_string())?;
        let mut got = 0u64;
        let mut buf = vec![0u8; 64 * 1024];
        loop {
            match stream.read_future(buf, glib::Priority::DEFAULT).await {
                Ok((next, 0)) => {
                    drop(next);
                    break;
                }
                Ok((next, n)) => {
                    if let Err(err) = std::io::Write::write_all(&mut file, &next[..n]) {
                        let _ = std::fs::remove_file(&partial);
                        return Err(err.to_string());
                    }
                    buf = next;
                    got += n as u64;
                    progress(got, total);
                }
                Err((_, err)) => {
                    let _ = std::fs::remove_file(&partial);
                    return Err(err.to_string());
                }
            }
        }
        drop(file);
        if got == 0 {
            let _ = std::fs::remove_file(&partial);
            return Err(format!("HTTP {code} for {url} was empty"));
        }
        std::fs::rename(&partial, dest).map_err(|e| {
            let _ = std::fs::remove_file(&partial);
            e.to_string()
        })?;
        Ok(())
    }

    /// True when `stored` is still inside `ttl_secs`.
    pub fn fresh(&self, method: &str, url: &str, body: Option<&str>, ttl_secs: u64) -> bool {
        if ttl_secs == 0 {
            return false;
        }
        read_cache(&self.dir.join(cache_key(method, url, body)), ttl_secs).is_some()
    }
}

async fn fetch(
    session: &soup::Session,
    method: &str,
    url: &str,
    body: Option<&str>,
) -> Result<Vec<u8>, String> {
    let message = soup::Message::new(method, url).map_err(|e| e.to_string())?;
    if let Some(body) = body {
        message.set_request_body_from_bytes(
            Some("application/json"),
            Some(&glib::Bytes::from(body.as_bytes())),
        );
    }
    let bytes = session
        .send_and_read_future(&message, glib::Priority::DEFAULT)
        .await
        .map_err(|e| e.to_string())?;
    let code = message.status_code();
    if !(200..300).contains(&code) {
        return Err(format!("HTTP {code} for {url}"));
    }
    Ok(bytes.to_vec())
}

pub(crate) fn cache_dir() -> PathBuf {
    glib::user_cache_dir().join("elavo-app-bay")
}

pub fn downloads_dir() -> PathBuf {
    let dir = cache_dir().join("downloads");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

fn cache_key(method: &str, url: &str, body: Option<&str>) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    method.hash(&mut hasher);
    url.hash(&mut hasher);
    body.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

fn read_cache(path: &std::path::Path, ttl_secs: u64) -> Option<Vec<u8>> {
    let meta = std::fs::read_to_string(path.with_extension("time")).ok()?;
    let written: u64 = meta.trim().parse().ok()?;
    let now = now_secs();
    if now.saturating_sub(written) > ttl_secs {
        return None;
    }
    read_stored(path)
}

fn read_stored(path: &std::path::Path) -> Option<Vec<u8>> {
    let bytes = std::fs::read(path).ok()?;
    if bytes.is_empty() {
        None
    } else {
        Some(bytes)
    }
}

fn write_cache(path: &std::path::Path, bytes: &[u8]) {
    let _ = std::fs::write(path, bytes);
    let _ = std::fs::write(path.with_extension("time"), now_secs().to_string());
}

fn partial_path(dest: &Path) -> std::path::PathBuf {
    let name = dest
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "download".into());
    dest.with_file_name(format!(".{name}.partial"))
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
