use crate::text::{cofl_message_text, no_color_codes};
use serde_json::Value;
use url::Url;

const SKY_COFL_AUTH_HOST: &str = "sky.coflnet.com";
const SKY_COFL_AUTH_PATH: &str = "/authmod";

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
        } else if let Some(connection_id) = cofl_auth_connection_id(candidate) {
            links.push(sky_cofl_auth_link(&connection_id));
        }
    }
    collect_auth_connection_ids_from_text(&cleaned, links);
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

fn cofl_auth_connection_id(candidate: &str) -> Option<String> {
    let (_, raw) = candidate
        .split_once("conId:")
        .or_else(|| candidate.split_once("conId="))?;
    let raw = raw.trim().trim_matches(|character: char| {
        matches!(
            character,
            '"' | '\'' | '`' | '<' | '>' | '[' | ']' | '(' | ')' | '{' | '}' | ',' | '.'
        )
    });
    let id = raw
        .chars()
        .take_while(|character| {
            character.is_ascii_alphanumeric()
                || matches!(*character, '%' | '_' | '-' | '.' | '+' | '/' | '=')
        })
        .collect::<String>();
    (!id.trim().is_empty()).then_some(id)
}

fn collect_auth_connection_ids_from_text(text: &str, links: &mut Vec<String>) {
    let marker = "conId:";
    let mut rest = text;
    while let Some((_, after_marker)) = rest.split_once(marker) {
        if let Some(connection_id) = cofl_auth_connection_id(&format!("{marker}{after_marker}")) {
            links.push(sky_cofl_auth_link(&connection_id));
        }
        rest = after_marker;
    }
}

fn sky_cofl_auth_link(connection_id: &str) -> String {
    let encoded =
        url::form_urlencoded::byte_serialize(connection_id.as_bytes()).collect::<String>();
    format!("https://sky.coflnet.com/authmod?conId={encoded}")
}
