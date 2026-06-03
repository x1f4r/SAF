//! `GET`/`PATCH /v1/config` — read and edit the operator `config.json5` over
//! the loopback dashboard API.
//!
//! Security model is an **allowlist** keyed on the top-level camelCase JSON
//! keys SAF emits. `GET` strips every secret-bearing key from the response, and
//! `PATCH` rejects (rather than silently dropping) any key that is not on the
//! safe list — so a typo or an attempt to overwrite the bot token, Cofl
//! session, or Discord webhook is reported back as `forbidden_fields`, never
//! written. SAFE nested objects (`skip`, `doNotBuy`, …) are merged wholesale.

use super::{ApiState, json_error};
use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use saf_core::SafConfig;
use serde_json::{Map, Value, json};

/// Top-level camelCase keys the dashboard is allowed to read and edit. These
/// are compared against the keys SAF emits when serializing [`SafConfig`]
/// (which is snake_case in Rust, camelCase in JSON via `rename_all`).
const SAFE_TOP_LEVEL_KEYS: &[&str] = &[
    "igns",
    "defaultIgn",
    "startDefaultOnly",
    "useCookie",
    "autoCookie",
    "angryCoopPrevention",
    "relist",
    "pingOnUpdate",
    "delay",
    "buyingReadyDelay",
    "waittime",
    "percentOfTarget",
    "listHours",
    "clickDelay",
    "bedSpam",
    "blockUselessMessages",
    "roundTo",
    "visitFriend",
    "webhookFormat",
    "branding",
    "skip",
    "doNotBuy",
    "doNotRelist",
    "autoRotate",
    "humanizer",
];

/// Secret-bearing keys: stripped from every `GET` response and rejected on
/// `PATCH`. Both the snake_case and camelCase spellings are listed so the strip
/// catches whatever key the serializer emits regardless of casing convention.
/// Note `webhookFormat` is SAFE (a message template); the Discord `webhook`
/// URL list is a secret and lives here.
const ALWAYS_REDACT_KEYS: &[&str] = &[
    "discordBot",
    "discord_bot",
    "session",
    "apiKey",
    "api_key",
    "webhook",
    "webhookUrl",
    "webhook_url",
    "sendAllFlips",
    "send_all_flips",
    "externalBackend",
    "external_backend",
    "discordId",
    "discord_id",
    "allowedIDs",
    "allowed_i_ds",
    "premiumToken",
    "premium_token",
    "sessionCookie",
    "session_cookie",
    "password",
    "secret",
];

/// Load the live config, serialize it, and strip every secret before returning.
pub(super) async fn get_config(State(state): State<ApiState>) -> Response {
    let config = match load_config(&state).await {
        Ok(config) => config,
        Err(response) => return response,
    };

    let mut value = match serde_json::to_value(&config) {
        Ok(Value::Object(map)) => map,
        Ok(_) | Err(_) => {
            return json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "serialize_error",
                "could not serialize config",
            );
        }
    };
    redact(&mut value);

    Json(json!({ "ok": true, "config": Value::Object(value) })).into_response()
}

/// Merge an allowlisted patch into the live config, validate it round-trips
/// through [`SafConfig`], and atomically rewrite `config.json5`.
pub(super) async fn patch_config(State(state): State<ApiState>, body: axum::body::Bytes) -> Response {
    // Parse the body ourselves so a malformed payload yields our `parse_error`
    // shape rather than axum's default rejection.
    let patch: Map<String, Value> = match serde_json::from_slice(&body) {
        Ok(Value::Object(map)) => map,
        Ok(_) | Err(_) => {
            return json_error(
                StatusCode::BAD_REQUEST,
                "parse_error",
                "Invalid JSON in request body",
            );
        }
    };

    // Closed allowlist: anything not explicitly SAFE is forbidden (this also
    // catches unknown/typo'd keys and every ALWAYS_REDACT key).
    let forbidden: Vec<String> = patch
        .keys()
        .filter(|key| !SAFE_TOP_LEVEL_KEYS.contains(&key.as_str()))
        .cloned()
        .collect();
    if !forbidden.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": "forbidden_fields",
                "message": format!(
                    "Cannot update forbidden fields: {}",
                    forbidden.join(", ")
                ),
                "forbidden": forbidden,
            })),
        )
            .into_response();
    }

    // Load current config as a Value, merge the patched top-level keys wholesale.
    let current = match load_config(&state).await {
        Ok(config) => config,
        Err(response) => return response,
    };
    let mut merged = match serde_json::to_value(&current) {
        Ok(Value::Object(map)) => map,
        Ok(_) | Err(_) => {
            return json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "serialize_error",
                "could not serialize config",
            );
        }
    };
    let mut updated: Vec<String> = Vec::with_capacity(patch.len());
    for (key, value) in patch {
        merged.insert(key.clone(), value);
        updated.push(key);
    }

    // Validate by round-tripping through SafConfig: a value that fails to
    // deserialize (wrong type, bad shape) is rejected before any write.
    let validated: SafConfig = match serde_json::from_value(Value::Object(merged)) {
        Ok(config) => config,
        Err(error) => {
            return json_error(
                StatusCode::BAD_REQUEST,
                "validation_error",
                format!("Config did not validate: {error}"),
            );
        }
    };

    let contents = match serde_json::to_string_pretty(&validated) {
        Ok(text) => text,
        Err(error) => {
            return json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "serialize_error",
                error.to_string(),
            );
        }
    };

    if let Err(error) =
        crate::config_write::write_config_atomic(state.config_path.as_ref(), contents).await
    {
        return json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "write_failed",
            error.to_string(),
        );
    }

    Json(json!({
        "ok": true,
        "message": "Config updated successfully",
        "updated": updated,
    }))
    .into_response()
}

/// Read and parse the live `config.json5`, mapping any I/O or parse failure to
/// a JSON error response.
async fn load_config(state: &ApiState) -> Result<SafConfig, Response> {
    let raw = tokio::fs::read_to_string(state.config_path.as_ref())
        .await
        .map_err(|error| {
            json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "config_read_failed",
                error.to_string(),
            )
        })?;
    SafConfig::from_json5_str(&raw).map_err(|error| {
        json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "config_parse_failed",
            error.to_string(),
        )
    })
}

/// Delete every [`ALWAYS_REDACT_KEYS`] entry from a serialized config object.
fn redact(value: &mut Map<String, Value>) {
    for key in ALWAYS_REDACT_KEYS {
        value.remove(*key);
    }
}

#[cfg(test)]
mod tests {
    use super::{ALWAYS_REDACT_KEYS, SAFE_TOP_LEVEL_KEYS, redact};
    use saf_core::SafConfig;
    use serde_json::Value;

    fn config_value() -> serde_json::Map<String, Value> {
        let config = SafConfig::default();
        match serde_json::to_value(&config).unwrap() {
            Value::Object(map) => map,
            other => panic!("expected object, got {other:?}"),
        }
    }

    #[test]
    fn get_output_omits_every_redacted_key() {
        let mut value = config_value();
        // The real config serializes the secret keys; confirm they exist first
        // for the ones SafConfig actually carries, then strip and re-check.
        assert!(value.contains_key("discordBot"));
        assert!(value.contains_key("session"));
        assert!(value.contains_key("webhook"));

        redact(&mut value);

        for key in ALWAYS_REDACT_KEYS {
            assert!(
                !value.contains_key(*key),
                "redacted key `{key}` leaked into GET output"
            );
        }
        // A SAFE template key with a confusingly similar name must survive.
        assert!(value.contains_key("webhookFormat"));
    }

    #[test]
    fn safe_field_passes_the_allowlist() {
        let patch: serde_json::Map<String, Value> =
            serde_json::from_str(r#"{ "useCookie": false, "delay": 300 }"#).unwrap();
        let forbidden: Vec<&String> = patch
            .keys()
            .filter(|key| !SAFE_TOP_LEVEL_KEYS.contains(&key.as_str()))
            .collect();
        assert!(forbidden.is_empty(), "SAFE fields were rejected: {forbidden:?}");

        // And it must round-trip into a valid config after merge.
        let mut merged = config_value();
        for (key, value) in patch {
            merged.insert(key, value);
        }
        let validated: SafConfig = serde_json::from_value(Value::Object(merged)).unwrap();
        assert!(!validated.use_cookie);
        assert_eq!(validated.delay, 300);
    }

    #[test]
    fn every_redact_key_is_forbidden_on_patch() {
        for key in ALWAYS_REDACT_KEYS {
            assert!(
                !SAFE_TOP_LEVEL_KEYS.contains(key),
                "secret key `{key}` must never be in the SAFE allowlist"
            );
        }
        // The headline secrets specifically must be rejected by the gate.
        for key in ["discordBot", "session", "webhook", "apiKey"] {
            assert!(
                !SAFE_TOP_LEVEL_KEYS.contains(&key),
                "secret `{key}` leaked into SAFE allowlist"
            );
        }
    }

    #[test]
    fn unknown_key_is_rejected() {
        // Closed allowlist: a key that is neither SAFE nor a known secret is
        // still forbidden.
        assert!(!SAFE_TOP_LEVEL_KEYS.contains(&"totallyMadeUpField"));
    }
}
