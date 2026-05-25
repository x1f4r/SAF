use url::Url;

pub const COFL_SOCKET_VERSION: &str = "1.5.1-af";

pub fn build_cofl_socket_link(
    command_or_link: &str,
    ign: &str,
    session_id: &str,
) -> Option<String> {
    let candidate = command_or_link
        .split_whitespace()
        .find(|part| is_websocket_link(part))?;
    let mut url = Url::parse(candidate).ok()?;
    let preserved_session = cofl_socket_session_id(candidate);
    if url.scheme() == "ws" {
        url.set_scheme("wss").ok()?;
    }
    let existing = url
        .query_pairs()
        .filter(|(key, _)| !matches!(key.as_ref(), "version" | "player" | "SId"))
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect::<Vec<_>>();
    {
        let mut query = url.query_pairs_mut();
        query.clear().extend_pairs(existing);
        query.append_pair("version", COFL_SOCKET_VERSION);
        query.append_pair("player", ign);
        let session_id = (!session_id.trim().is_empty())
            .then(|| session_id.trim().to_string())
            .or(preserved_session);
        if let Some(session_id) = session_id {
            query.append_pair("SId", &session_id);
        }
    }
    Some(url.to_string())
}

pub fn cofl_socket_session_id(command_or_link: &str) -> Option<String> {
    let candidate = command_or_link
        .split_whitespace()
        .find(|part| is_websocket_link(part))?;
    Url::parse(candidate)
        .ok()?
        .query_pairs()
        .find(|(key, _)| key == "SId")
        .map(|(_, value)| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn is_websocket_link(value: &str) -> bool {
    let value = value.to_ascii_lowercase();
    value.starts_with("ws://") || value.starts_with("wss://")
}

pub fn redact_cofl_socket_link(link: &str) -> String {
    if let Ok(mut url) = Url::parse(link) {
        if url.query_pairs().any(|(key, _)| key == "SId") {
            let pairs = url
                .query_pairs()
                .map(|(key, value)| {
                    let value = if key == "SId" {
                        "[redacted]".into()
                    } else {
                        value
                    };
                    (key.into_owned(), value.into_owned())
                })
                .collect::<Vec<_>>();
            url.query_pairs_mut().clear().extend_pairs(pairs);
        }
        return url.to_string();
    }

    link.replace("SId=", "SId=[redacted]")
}
