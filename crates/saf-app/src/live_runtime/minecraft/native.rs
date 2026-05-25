#[cfg(feature = "live-minecraft")]
use super::super::native_minecraft_enabled;
use super::clients::LiveMinecraftClientBundle;
#[cfg(feature = "live-minecraft")]
use crate::config_session::account_env_key;
#[cfg(feature = "live-minecraft")]
use anyhow::Context;
use anyhow::Result;
use saf_core::AccountId;
use saf_minecraft::RecordedMinecraftClient;
use std::sync::Arc;

pub(in crate::live_runtime) async fn connect_minecraft_client(
    account: &AccountId,
) -> Result<LiveMinecraftClientBundle> {
    #[cfg(feature = "live-minecraft")]
    {
        if native_minecraft_enabled() {
            let server = std::env::var("SAF_MINECRAFT_SERVER")
                .ok()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| "mc.hypixel.net".into());
            let cache_key = std::env::var(account_env_key("SAF_MICROSOFT_CACHE", account))
                .ok()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| account.to_string());
            let config = saf_minecraft::azalea_native::AzaleaClientConfig::microsoft(
                account.clone(),
                server,
                cache_key,
            );
            let client = Arc::new(
                saf_minecraft::azalea_native::AzaleaMinecraftClient::connect(config)
                    .await
                    .with_context(|| format!("connecting Minecraft account {account}"))?,
            );
            return Ok(LiveMinecraftClientBundle {
                minecraft: client.clone(),
                inventory_provider: Some(client),
            });
        }
    }

    Ok(LiveMinecraftClientBundle {
        minecraft: Arc::new(RecordedMinecraftClient::new(account.clone())),
        inventory_provider: None,
    })
}
