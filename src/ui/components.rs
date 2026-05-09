// Dioxus components are defined inline in mod.rs using RSX.
// This file is kept for shared utility functions.

use crate::types::UserStatus;

/// Get 2-char initials from a name.
pub fn initials(name: &str) -> String {
    let chars: Vec<char> = name.chars().collect();
    if chars.len() >= 2 {
        format!("{}{}", chars[0].to_lowercase(), chars[1].to_lowercase())
    } else {
        chars.first().map(|c| c.to_lowercase().to_string()).unwrap_or_else(|| "?".into())
    }
}

/// Get CSS class for status dot.
pub fn status_class(status: UserStatus) -> &'static str {
    match status {
        UserStatus::Online => "online",
        UserStatus::Away => "away",
        UserStatus::DoNotDisturb => "dnd",
        UserStatus::Offline => "offline",
    }
}

/// Format a file size.
pub fn format_size(bytes: u64) -> String {
    if bytes > 1_000_000 {
        format!("{:.1} MB", bytes as f64 / 1_000_000.0)
    } else {
        format!("{:.0} KB", bytes as f64 / 1_000.0)
    }
}

/// Simple markdown to HTML conversion for message content.
/// Supports: **bold**, *italic*, `code`, ~~strikethrough~~, [links](url)
pub fn markdown_to_html(text: &str) -> String {
    let mut result = text.to_string();

    // Escape HTML first
    result = result.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");

    // Code blocks (backtick)
    result = regex_replace(&result, r"`([^`]+)`", "<code>$1</code>");

    // Bold
    result = regex_replace(&result, r"\*\*([^*]+)\*\*", "<strong>$1</strong>");

    // Italic
    result = regex_replace(&result, r"\*([^*]+)\*", "<em>$1</em>");

    // Strikethrough
    result = regex_replace(&result, r"~~([^~]+)~~", "<del>$1</del>");

    // URLs — auto-link bare URLs
    result = regex_replace(&result, r"(https?://[^\s<]+)", r#"<a href="$1" target="_blank">$1</a>"#);

    result
}

fn regex_replace(text: &str, pattern: &str, replacement: &str) -> String {
    // Simple regex-like replacement without pulling in the regex crate
    // This handles the most common patterns
    let mut result = String::new();
    let mut chars = text.chars().peekable();

    match pattern {
        r"`([^`]+)`" => {
            while let Some(c) = chars.next() {
                if c == '`' {
                    let mut inner = String::new();
                    let mut found_end = false;
                    for c2 in chars.by_ref() {
                        if c2 == '`' { found_end = true; break; }
                        inner.push(c2);
                    }
                    if found_end && !inner.is_empty() {
                        result.push_str(&format!("<code>{}</code>", inner));
                    } else {
                        result.push('`');
                        result.push_str(&inner);
                    }
                } else {
                    result.push(c);
                }
            }
        }
        r"\*\*([^*]+)\*\*" => {
            let text_bytes = text.as_bytes();
            let mut i = 0;
            while i < text_bytes.len() {
                if i + 1 < text_bytes.len() && text_bytes[i] == b'*' && text_bytes[i + 1] == b'*' {
                    let start = i + 2;
                    if let Some(end) = text[start..].find("**") {
                        result.push_str(&format!("<strong>{}</strong>", &text[start..start + end]));
                        i = start + end + 2;
                        continue;
                    }
                }
                result.push(text_bytes[i] as char);
                i += 1;
            }
        }
        r"\*([^*]+)\*" => {
            let text_bytes = text.as_bytes();
            let mut i = 0;
            while i < text_bytes.len() {
                if text_bytes[i] == b'*' && (i == 0 || text_bytes[i - 1] != b'*') && (i + 1 >= text_bytes.len() || text_bytes[i + 1] != b'*') {
                    let start = i + 1;
                    if let Some(end_pos) = text[start..].find('*') {
                        if start + end_pos < text_bytes.len() && (start + end_pos + 1 >= text_bytes.len() || text_bytes[start + end_pos + 1] != b'*') {
                            result.push_str(&format!("<em>{}</em>", &text[start..start + end_pos]));
                            i = start + end_pos + 1;
                            continue;
                        }
                    }
                }
                result.push(text_bytes[i] as char);
                i += 1;
            }
        }
        r"~~([^~]+)~~" => {
            result = text.to_string();
            while let Some(start) = result.find("~~") {
                if let Some(end) = result[start + 2..].find("~~") {
                    let inner = result[start + 2..start + 2 + end].to_string();
                    result = format!("{}<del>{}</del>{}", &result[..start], inner, &result[start + 2 + end + 2..]);
                } else {
                    break;
                }
            }
        }
        _ if pattern.contains("https?://") => {
            // Auto-link URLs
            let mut last = 0;
            let text_str = text;
            for (i, _) in text_str.match_indices("http") {
                result.push_str(&text_str[last..i]);
                let url_end = text_str[i..].find(|c: char| c.is_whitespace() || c == '<').unwrap_or(text_str.len() - i);
                let url = &text_str[i..i + url_end];
                result.push_str(&format!(r#"<a href="{}" target="_blank">{}</a>"#, url, url));
                last = i + url_end;
            }
            result.push_str(&text_str[last..]);
        }
        _ => {
            result = text.to_string();
        }
    }

    result
}
