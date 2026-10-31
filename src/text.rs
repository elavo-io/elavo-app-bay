//! Plain text from Flathub's HTML, and the short permission phrases.

/// Turn an AppStream HTML fragment into readable text.
pub fn html_to_text(html: &str) -> String {
    let mut out = String::new();
    let mut tag = String::new();
    let mut in_tag = false;
    for c in html.chars() {
        if in_tag {
            if c == '>' {
                in_tag = false;
                let name = tag
                    .split_whitespace()
                    .next()
                    .unwrap_or("")
                    .to_ascii_lowercase();
                let name = name.trim_start_matches('/');
                match name {
                    "li" if !tag.starts_with('/') => out.push_str("\n• "),
                    "br" | "p" | "div" | "ul" | "ol" | "h1" | "h2" | "h3" | "tr"
                        if !out.ends_with('\n') =>
                    {
                        out.push('\n');
                    }
                    _ => {}
                }
                tag.clear();
            } else {
                tag.push(c);
            }
        } else if c == '<' {
            in_tag = true;
        } else {
            out.push(c);
        }
    }
    let decoded = decode_entities(&out);
    let mut collapsed = String::new();
    let mut blank = 0;
    for line in decoded.lines() {
        let line = line.trim();
        if line.is_empty() {
            blank += 1;
            if blank <= 1 && !collapsed.is_empty() {
                collapsed.push('\n');
            }
        } else {
            blank = 0;
            if !collapsed.is_empty() && !collapsed.ends_with('\n') {
                collapsed.push(' ');
            }
            collapsed.push_str(line);
            collapsed.push('\n');
        }
    }
    collapsed.trim().to_owned()
}

fn decode_entities(text: &str) -> String {
    text.replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
}

pub fn format_size(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["bytes", "KB", "MB", "GB"];
    let mut size = bytes as f64;
    let mut unit = 0;
    while size >= 1024.0 && unit < UNITS.len() - 1 {
        size /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} {}", UNITS[0])
    } else {
        format!("{size:.1} {}", UNITS[unit])
    }
}

/// A permission worth telling a person about. Display and graphics are left
/// out: almost every Flatpak has them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Permission {
    Network,
    AllFiles,
    Home,
    Downloads,
    Devices,
}

pub fn permissions(value: &serde_json::Value) -> Vec<Permission> {
    let Some(perms) = value
        .get("metadata")
        .and_then(|m| m.get("permissions"))
        .or_else(|| value.get("permissions"))
    else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let list = |key: &str| -> Vec<String> {
        perms
            .get(key)
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default()
    };
    if list("shared").iter().any(|s| s == "network") {
        out.push(Permission::Network);
    }
    if list("devices").iter().any(|s| s == "all") {
        out.push(Permission::Devices);
    }
    let files = list("filesystems");
    if files.iter().any(|s| s == "host" || s.starts_with("host:")) {
        out.push(Permission::AllFiles);
    } else if files.iter().any(|s| s == "home" || s.starts_with("home:")) {
        out.push(Permission::Home);
    }
    if files.iter().any(|s| s.contains("xdg-download")) {
        out.push(Permission::Downloads);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_a_release_note() {
        let text = html_to_text("<p>Hello</p><ul><li>One</li><li>Two</li></ul>");
        assert!(text.contains("Hello"), "{text}");
        assert!(text.contains("• One"), "{text}");
        assert!(!text.contains('<'), "{text}");
    }

    #[test]
    fn reads_permissions() {
        let value = serde_json::json!({
            "metadata": {
                "permissions": {
                    "shared": ["network", "ipc"],
                    "filesystems": ["host"],
                    "devices": ["dri"]
                }
            }
        });
        let got = permissions(&value);
        assert!(got.contains(&Permission::Network));
        assert!(got.contains(&Permission::AllFiles));
        assert!(!got.contains(&Permission::Devices));
    }
}
