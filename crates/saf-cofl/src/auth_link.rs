use crate::text::{cofl_message_text, no_color_codes};
use serde_json::Value;
use url::Url;

const SKY_COFL_AUTH_HOST: &str = "sky.coflnet.com";
const SKY_COFL_AUTH_PATH: &str = "/authmod";

/// Extract the SkyCofl login links Coflnet actually sends: full
/// `https://sky.coflnet.com/authmod?...` URLs that carry the `mcid` and the
/// authoritative (base64) `conId`. Opening one of these and logging in binds the
/// session.
///
/// We deliberately do NOT synthesize a link from a bare `conId:` marker. Coflnet
/// prints a *diagnostic* connection id in its greeting ("Attempting to load your
/// settings ... conId: <32 hex>", "copy that if you encounter an error"). That id
/// is for error reports, not authorization — its envelope arrives before the real
/// login link, and building `authmod?conId=<that>` produced a link that looked
/// valid but never bound the session, stranding the operator's login.
pub fn cofl_auth_links(data: &Value) -> Vec<String> {
    let mut links = Vec::new();
    collect_auth_links_from_value(data, &mut links);
    if let Some(text) = cofl_message_text(data) {
        collect_auth_links_from_text(&text, &mut links);
    }
    links.sort();
    links.dedup();
    links
}

fn collect_auth_links_from_value(value: &Value, links: &mut Vec<String>) {
    match value {
        Value::String(text) => collect_auth_links_from_text(text, links),
        Value::Array(values) => {
            for value in values {
                collect_auth_links_from_value(value, links);
            }
        }
        Value::Object(object) => {
            for value in object.values() {
                collect_auth_links_from_value(value, links);
            }
        }
        Value::Number(_) | Value::Bool(_) | Value::Null => {}
    }
}

pub fn collect_auth_links_from_text(text: &str, links: &mut Vec<String>) {
    let cleaned = no_color_codes(text);
    for token in cleaned.split_whitespace() {
        let candidate = trim_url_token(token);
        if is_sky_cofl_auth_link(candidate) {
            links.push(candidate.to_string());
        }
    }
}

fn trim_url_token(token: &str) -> &str {
    token.trim_matches(|ch: char| {
        matches!(
            ch,
            '"' | '\'' | '`' | '<' | '>' | '[' | ']' | '(' | ')' | '{' | '}' | ',' | '.'
        )
    })
}

fn is_sky_cofl_auth_link(candidate: &str) -> bool {
    let Ok(url) = Url::parse(candidate) else {
        return false;
    };
    matches!(url.scheme(), "https" | "http")
        && url.host_str().is_some_and(|host| {
            host.eq_ignore_ascii_case(SKY_COFL_AUTH_HOST)
                || host.ends_with(&format!(".{SKY_COFL_AUTH_HOST}"))
        })
        && url.path().eq_ignore_ascii_case(SKY_COFL_AUTH_PATH)
        && url
            .query_pairs()
            .any(|(key, value)| key.eq_ignore_ascii_case("conId") && !value.trim().is_empty())
}
