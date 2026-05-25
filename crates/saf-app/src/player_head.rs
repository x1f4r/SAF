use anyhow::{Context, Result};
use base64::Engine;
mod config_patch;
use saf_core::{AccountId, SafConfig};
use serde::Deserialize;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[cfg(test)]
use config_patch::patch_player_head_version;
use config_patch::update_config_version;

const DEFAULT_PLAYER_HEAD_TEMPLATE: &str =
    "https://crafthead.net/cube/{texture}?size=180&v={version}";

#[derive(Clone, Debug)]
pub struct PlayerHeadRefresher {
    config: PlayerHeadConfig,
    config_path: PathBuf,
    client: reqwest::Client,
}

impl PlayerHeadRefresher {
    pub fn live(config: &SafConfig, config_path: PathBuf) -> Self {
        Self {
            config: PlayerHeadConfig::from_config(config),
            config_path,
            client: reqwest::Client::new(),
        }
    }

    #[cfg(test)]
    fn new(config: PlayerHeadConfig, config_path: PathBuf) -> Self {
        Self {
            config,
            config_path,
            client: reqwest::Client::new(),
        }
    }

    pub async fn refresh(&self, accounts: &[AccountId]) -> Result<PlayerHeadRefreshReport> {
        let version = current_version();
        let config_updated = update_config_version(&self.config_path, &version)
            .await
            .with_context(|| {
                format!(
                    "updating branding.playerHeadVersion in {}",
                    self.config_path.display()
                )
            })?;
        let mut report = PlayerHeadRefreshReport {
            version: version.clone(),
            config_updated,
            refreshed: Vec::new(),
            skipped: Vec::new(),
        };

        for account in accounts {
            match self.refresh_account(account, &version).await {
                Ok(Some(entry)) => report.refreshed.push(entry),
                Ok(None) => report.skipped.push(account.to_string()),
                Err(error) => {
                    tracing::warn!(account = %account, error = %error, "failed to refresh player head");
                    report.skipped.push(account.to_string());
                }
            }
        }

        Ok(report)
    }

    async fn refresh_account(
        &self,
        account: &AccountId,
        version: &str,
    ) -> Result<Option<PlayerHeadRefreshEntry>> {
        let profile = fetch_mojang_profile(&self.client, account.as_str()).await?;
        let Some(profile) = profile else {
            return Ok(None);
        };
        let texture_hash = fetch_skin_texture_hash(&self.client, &profile.id).await?;
        let Some(head_url) = build_player_head_url(
            &profile.id,
            version,
            &profile.name,
            texture_hash.as_deref(),
            &self.config.url_template,
        ) else {
            return Ok(None);
        };
        Ok(Some(PlayerHeadRefreshEntry {
            username: profile.name,
            uuid: clean_uuid(&profile.id),
            texture_hash,
            head_url,
        }))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlayerHeadConfig {
    url_template: String,
}

impl PlayerHeadConfig {
    fn from_config(config: &SafConfig) -> Self {
        let url_template = std::env::var("SAF_PLAYER_HEAD_URL_TEMPLATE")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| config.branding.player_head_url_template.clone());
        Self {
            url_template: if url_template.trim().is_empty() {
                DEFAULT_PLAYER_HEAD_TEMPLATE.to_string()
            } else {
                url_template
            },
        }
    }

    #[cfg(test)]
    fn new(url_template: impl Into<String>) -> Self {
        Self {
            url_template: url_template.into(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlayerHeadRefreshReport {
    pub version: String,
    pub config_updated: bool,
    pub refreshed: Vec<PlayerHeadRefreshEntry>,
    pub skipped: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlayerHeadRefreshEntry {
    pub username: String,
    pub uuid: String,
    pub texture_hash: Option<String>,
    pub head_url: String,
}

pub fn build_player_head_url(
    uuid: &str,
    version: &str,
    username: &str,
    texture_hash: Option<&str>,
    template: &str,
) -> Option<String> {
    let clean_uuid = clean_uuid(uuid);
    if clean_uuid.is_empty() {
        return None;
    }
    let texture = texture_hash
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(clean_uuid.as_str());
    let template = if template.trim().is_empty() {
        DEFAULT_PLAYER_HEAD_TEMPLATE
    } else {
        template
    };
    Some(
        template
            .replace("{texture}", &url_encode(texture))
            .replace("{uuid}", &url_encode(&clean_uuid))
            .replace("{username}", &url_encode(username))
            .replace("{version}", &url_encode(version)),
    )
}

async fn fetch_mojang_profile(
    client: &reqwest::Client,
    username: &str,
) -> Result<Option<MojangProfile>> {
    let username = username.trim();
    if username.is_empty() {
        return Ok(None);
    }
    let url = format!(
        "https://api.mojang.com/users/profiles/minecraft/{}",
        url_encode(username)
    );
    let response = client
        .get(url)
        .timeout(Duration::from_secs(5))
        .send()
        .await?;
    if matches!(
        response.status(),
        reqwest::StatusCode::NOT_FOUND | reqwest::StatusCode::NO_CONTENT
    ) {
        return Ok(None);
    }
    Ok(Some(response.error_for_status()?.json().await?))
}

async fn fetch_skin_texture_hash(client: &reqwest::Client, uuid: &str) -> Result<Option<String>> {
    let uuid = clean_uuid(uuid);
    if uuid.is_empty() {
        return Ok(None);
    }
    let url =
        format!("https://sessionserver.mojang.com/session/minecraft/profile/{uuid}?unsigned=false");
    let response = client
        .get(url)
        .timeout(Duration::from_secs(5))
        .send()
        .await?;
    if matches!(
        response.status(),
        reqwest::StatusCode::NOT_FOUND | reqwest::StatusCode::NO_CONTENT
    ) {
        return Ok(None);
    }
    let profile = response
        .error_for_status()?
        .json::<MojangSessionProfile>()
        .await?;
    let Some(property) = profile
        .properties
        .iter()
        .find(|property| property.name == "textures")
    else {
        return Ok(None);
    };
    let decoded = base64::engine::general_purpose::STANDARD.decode(&property.value)?;
    let textures = serde_json::from_slice::<TexturePayload>(&decoded)?;
    Ok(textures
        .textures
        .skin
        .and_then(|skin| skin.url.rsplit('/').next().map(ToOwned::to_owned)))
}

fn clean_uuid(uuid: &str) -> String {
    uuid.chars().filter(|ch| *ch != '-').collect()
}

fn url_encode(value: &str) -> String {
    let mut encoded = String::new();
    for byte in value.as_bytes() {
        if matches!(
            *byte,
            b'A'..=b'Z'
                | b'a'..=b'z'
                | b'0'..=b'9'
                | b'-'
                | b'_'
                | b'.'
                | b'!'
                | b'~'
                | b'*'
                | b'\''
                | b'('
                | b')'
        ) {
            encoded.push(*byte as char);
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

fn current_version() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .to_string()
}

#[derive(Debug, Deserialize)]
struct MojangProfile {
    id: String,
    name: String,
}

#[derive(Debug, Deserialize)]
struct MojangSessionProfile {
    #[serde(default)]
    properties: Vec<MojangProperty>,
}

#[derive(Debug, Deserialize)]
struct MojangProperty {
    name: String,
    value: String,
}

#[derive(Debug, Deserialize)]
struct TexturePayload {
    textures: TextureSet,
}

#[derive(Debug, Deserialize)]
struct TextureSet {
    #[serde(rename = "SKIN")]
    skin: Option<SkinTexture>,
}

#[derive(Debug, Deserialize)]
struct SkinTexture {
    url: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn player_head_urls_match_node_template_semantics() {
        let url = build_player_head_url(
            "abcdefab-cdef-abcd-efab-cdefabcdefab",
            "skin-refresh-1",
            "Main Account",
            Some("texture-hash"),
            DEFAULT_PLAYER_HEAD_TEMPLATE,
        )
        .unwrap();

        assert_eq!(
            url,
            "https://crafthead.net/cube/texture-hash?size=180&v=skin-refresh-1"
        );
        assert_eq!(
            build_player_head_url(
                "abcdefab-cdef-abcd-efab-cdefabcdefab",
                "v 1",
                "Main Account",
                None,
                "https://example.test/{uuid}/{username}/{texture}?v={version}",
            )
            .unwrap(),
            "https://example.test/abcdefabcdefabcdefabcdefabcdefab/Main%20Account/abcdefabcdefabcdefabcdefabcdefab?v=v%201"
        );
    }

    #[test]
    fn config_patch_preserves_json5_surroundings() {
        let raw = r#"{
          note: "playerHeadVersion: not the config field",
          // playerHeadVersion: "comment-only"
          // Keep this comment
          branding: {
            name: "SAF",
            playerHeadVersion: "old-version",
            playerHeadUrlTemplate: "https://crafthead.net/cube/{texture}?size=180&v={version}"
          }
        }"#;

        let patched = patch_player_head_version(raw, "new-version").unwrap();

        assert!(patched.contains("// Keep this comment"));
        assert!(patched.contains(r#"note: "playerHeadVersion: not the config field""#));
        assert!(patched.contains(r#"// playerHeadVersion: "comment-only""#));
        assert!(patched.contains(r#"playerHeadVersion: "new-version""#));
        assert!(patched.contains("playerHeadUrlTemplate"));
    }

    #[tokio::test]
    async fn config_update_sets_branding_player_head_version() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("config.json5");
        tokio::fs::write(
            &path,
            r#"{
              // Preserve comments and formatting
              branding: {
                name: "SAF",
                playerHeadVersion: ""
              }
            }"#,
        )
        .await
        .unwrap();
        let refresher = PlayerHeadRefresher::new(
            PlayerHeadConfig::new(DEFAULT_PLAYER_HEAD_TEMPLATE),
            path.clone(),
        );

        let report = refresher.refresh(&[]).await.unwrap();
        let updated = tokio::fs::read_to_string(path).await.unwrap();

        assert!(report.config_updated);
        assert!(updated.contains("// Preserve comments and formatting"));
        assert!(updated.contains(&format!(r#"playerHeadVersion: "{}""#, report.version)));
    }
}
