use anyhow::{Context, Result};
use saf_app::config_session::{account_env_key, ensure_cofl_session};
use saf_core::ports::CoflClient;
use saf_core::{AccountEnvironment, AccountId, SafConfig, config_with_account_environment};
use serde::Serialize;
use std::path::Path;
use std::time::Duration;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CoflAuthLinkReport {
    #[serde(rename = "type")]
    kind: &'static str,
    account: String,
    link: Option<String>,
    links: Vec<String>,
    auth_required: bool,
    session_persisted: bool,
    socket_link: String,
    message: String,
}

pub(crate) async fn print_cofl_auth_link(
    config_path: &Path,
    account: Option<String>,
    socket_link: Option<String>,
    timeout_ms: u64,
) -> Result<()> {
    let config = SafConfig::from_path(config_path)
        .with_context(|| format!("loading {}", config_path.display()))?;
    let mut config = config_with_account_environment(&config, &AccountEnvironment::current());
    let account = account
        .or_else(|| config.default_account())
        .context("account is required; pass an account or configure defaultIgn")?;
    let account = AccountId::new(account).context("account is required")?;
    let explicit_socket_has_session = socket_link
        .as_deref()
        .is_some_and(|link| saf_cofl::cofl_socket_session_id(link).is_some());
    let generated_session = if explicit_socket_has_session {
        false
    } else {
        ensure_cofl_session(
            &mut config,
            std::slice::from_ref(&account),
            config_path,
            |name| std::env::var(name).ok(),
        )?
        .is_some()
    };
    let socket_link = resolve_socket_link(&config, &account, socket_link.as_deref())?;
    let redacted_socket_link = saf_cofl::redact_cofl_socket_link(&socket_link);
    let client = saf_cofl::ws_client::CoflWebSocketClient::connect(account.clone(), &socket_link)
        .await
        .with_context(|| format!("connecting SkyCofl websocket for {account}"))?;

    let timeout = Duration::from_millis(timeout_ms.max(1));
    let mut auth_required = false;
    let mut saw_account_tier = false;
    let mut requested_settings = false;
    let started = tokio::time::Instant::now();
    while started.elapsed() < timeout {
        let remaining = timeout.saturating_sub(started.elapsed());
        let envelope = match tokio::time::timeout(remaining, client.next_envelope()).await {
            Ok(Ok(Some(envelope))) => envelope,
            Ok(Ok(None)) => break,
            Ok(Err(error)) => return Err(error).context("reading SkyCofl auth envelope"),
            Err(_) => break,
        };

        {
            let links = envelope.auth_links();
            if !links.is_empty() {
                print_report(CoflAuthLinkReport {
                    kind: "skyCoflAuthLink",
                    account: account.to_string(),
                    link: links.first().cloned(),
                    links,
                    auth_required: true,
                    session_persisted: generated_session,
                    socket_link: redacted_socket_link,
                    message: "Open the link, log in with Google, then keep this session value in config.json5.".to_string(),
                })?;
                return Ok(());
            }
        }

        auth_required |= envelope.logged_out_settings_recovery_command().is_some();
        saw_account_tier |= envelope
            .telemetry_update()
            .is_some_and(|update| update.cofl_tier.is_some());
        if !requested_settings {
            if let Err(error) = client.send_command(&account, "/cofl get json").await {
                tracing::debug!(account = %account, error = %error, "SkyCofl auth helper could not request settings json");
            }
            requested_settings = true;
        }
    }

    if saw_account_tier && !auth_required {
        print_report(CoflAuthLinkReport {
            kind: "skyCoflAuthAlreadyReady",
            account: account.to_string(),
            link: None,
            links: Vec::new(),
            auth_required: false,
            session_persisted: generated_session,
            socket_link: redacted_socket_link,
            message: "SkyCofl did not send a login link; this session already looks authorized."
                .to_string(),
        })?;
        return Ok(());
    }

    print_report(CoflAuthLinkReport {
        kind: "skyCoflAuthLinkMissing",
        account: account.to_string(),
        link: None,
        links: Vec::new(),
        auth_required,
        session_persisted: generated_session,
        socket_link: redacted_socket_link,
        message: "SkyCofl did not send an authmod link before the timeout.".to_string(),
    })?;
    anyhow::bail!("SkyCofl did not send an authmod link before the timeout")
}

fn resolve_socket_link(
    config: &SafConfig,
    account: &AccountId,
    explicit_link: Option<&str>,
) -> Result<String> {
    let configured_session = cofl_session(config);
    if let Some(link) = explicit_link.map(str::trim).filter(|link| !link.is_empty()) {
        return normalize_socket_link(link, account, &configured_session)
            .with_context(|| "explicit SkyCofl socket link is invalid or missing a session");
    }

    let account_socket_key = account_env_key("SAF_COFL_SOCKET", account);
    if let Ok(link) = std::env::var(&account_socket_key)
        && !link.trim().is_empty()
    {
        return normalize_socket_link(&link, account, &configured_session)
            .with_context(|| format!("{account_socket_key} is invalid or missing a session"));
    }

    let session = configured_session.trim();
    if session.is_empty() {
        anyhow::bail!("SkyCofl session is required; set config.session or SAF_COFL_SESSION")
    }
    saf_cofl::build_cofl_socket_link("wss://sky.coflnet.com/modsocket", account.as_str(), session)
        .context("building default SkyCofl socket link")
}

fn normalize_socket_link(
    link: &str,
    account: &AccountId,
    configured_session: &str,
) -> Option<String> {
    let session = if saf_cofl::cofl_socket_session_id(link).is_some() {
        ""
    } else {
        configured_session
    };
    saf_cofl::build_cofl_socket_link(link, account.as_str(), session)
}

fn cofl_session(config: &SafConfig) -> String {
    std::env::var("SAF_COFL_SESSION")
        .ok()
        .map(|session| session.trim().to_string())
        .filter(|session| !session.is_empty())
        .or_else(|| {
            let session = config.session.trim();
            (!session.is_empty()).then(|| session.to_string())
        })
        .unwrap_or_default()
}

fn print_report(report: CoflAuthLinkReport) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}
