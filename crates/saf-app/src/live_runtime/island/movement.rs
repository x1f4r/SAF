use super::super::windows::visit_window_disallows_guests;
use super::super::{LiveRuntime, is_bad_modification_message, native_minecraft_enabled};
use anyhow::{Context, Result};
use saf_core::AccountId;
use saf_core::ports::{MinecraftAction, MinecraftClient};
use serde_json::Value;
use std::collections::BTreeSet;
use std::time::Instant;

const LOCRAW_RESPONSE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);
const MAX_LOCRAW_TIMEOUTS_BEFORE_RECONNECT: u8 = 3;
const SKYBLOCK_UNAVAILABLE_BACKOFF: std::time::Duration = std::time::Duration::from_secs(60);

impl LiveRuntime {
    pub(in crate::live_runtime) async fn drive_island_checks_once(&mut self) -> Result<()> {
        let now = Instant::now();
        let running = self.running_account_set();
        let command_ready = self
            .minecraft_command_ready_accounts(&running, native_minecraft_enabled())
            .await;
        let mut reconnect_after_stale_locraw = Vec::new();
        for (account, state) in &mut self.island_states {
            if !running.contains(account) {
                continue;
            }
            if state.require_ready_for_market
                && !state.ready
                && state.locraw_response_timed_out(now, LOCRAW_RESPONSE_TIMEOUT)
                && !state.bad_modification_backoff_active(now)
                && !state.movement_backoff_active(now)
            {
                let timeout_count = state.retry_locraw_after_timeout(now);
                if timeout_count >= MAX_LOCRAW_TIMEOUTS_BEFORE_RECONNECT {
                    reconnect_after_stale_locraw.push((account.clone(), timeout_count));
                    continue;
                }
                tracing::warn!(
                    account = %account,
                    timeout_count,
                    timeout_ms = LOCRAW_RESPONSE_TIMEOUT.as_millis(),
                    "startup locraw response timed out; retrying readiness check"
                );
            }
            if running.contains(account)
                && command_ready.contains(account)
                && state.require_ready_for_market
                && !state.ready
                && state.locraw_due.is_none()
                && !state.awaiting_locraw
                && !state.bad_modification_backoff_active(now)
                && !state.movement_backoff_active(now)
            {
                tracing::info!(
                    account = %account,
                    locraw_delay_ms = state.locraw_delay.as_millis(),
                    "scheduling startup locraw check for unready Minecraft account"
                );
                state.schedule_locraw(now);
            }
        }

        for (account, timeout_count) in reconnect_after_stale_locraw {
            self.handle_minecraft_action_failure(
                &account,
                "startup locraw response timeout",
                anyhow::anyhow!(
                    "startup locraw response timed out {timeout_count} times without a chat response"
                ),
            )
            .await?;
        }

        let due = self
            .island_states
            .iter_mut()
            .filter_map(|(account, state)| {
                (running.contains(account)
                    && command_ready.contains(account)
                    && state.take_due_locraw(now))
                .then(|| account.clone())
            })
            .collect::<Vec<_>>();

        for account in due {
            let Some(client) = self.minecraft_clients.get(&account).cloned() else {
                continue;
            };
            tracing::info!(
                account = %account,
                "requesting startup locraw for Minecraft account"
            );
            if let Err(error) = client
                .perform(MinecraftAction::Chat("/locraw".to_string()))
                .await
                .with_context(|| format!("requesting locraw for {account}"))
            {
                self.handle_minecraft_action_failure(&account, "locraw request", error)
                    .await?;
            }
        }
        Ok(())
    }

    pub(in crate::live_runtime) async fn minecraft_command_ready_accounts(
        &self,
        running: &BTreeSet<AccountId>,
        require_play_state: bool,
    ) -> BTreeSet<AccountId> {
        let mut ready = BTreeSet::new();
        for account in running {
            let Some(managed) = self.managed_minecraft.get(account) else {
                ready.insert(account.clone());
                continue;
            };
            if managed.has_active_runtime().await {
                if require_play_state && !self.minecraft_ready_accounts.contains(account) {
                    continue;
                }
                ready.insert(account.clone());
            }
        }
        ready
    }

    pub(in crate::live_runtime) fn schedule_island_locraw(&mut self, account: &AccountId) {
        if let Some(state) = self.island_states.get_mut(account) {
            state.schedule_locraw(Instant::now());
        }
    }

    pub(in crate::live_runtime) async fn consume_visit_friend_window(
        &mut self,
        account: &AccountId,
        client: &dyn MinecraftClient,
    ) -> Result<bool> {
        let visit_pending = self
            .island_states
            .get(account)
            .is_some_and(|state| state.visit_pending);
        if !visit_pending {
            return Ok(false);
        }
        let window = self
            .active_windows
            .lock()
            .map_err(|_| anyhow::anyhow!("active window lock poisoned"))?
            .get(account)
            .cloned();
        let Some(window) = window else {
            return Ok(false);
        };
        if visit_window_disallows_guests(&window) {
            if let Some(state) = self.island_states.get_mut(account) {
                state.disable_visit_friend();
                state.mark_moving();
                state.schedule_locraw(Instant::now());
            }
            client.perform(MinecraftAction::CloseWindow).await?;
            client
                .perform(MinecraftAction::Chat("/hub".to_string()))
                .await?;
            self.clear_active_window_cache(account)?;
            return Ok(true);
        }
        let slot = window
            .resolve_slot(&["visit", "island", "travel", "confirm"], Some(11))
            .unwrap_or(11);
        if let Some(state) = self.island_states.get_mut(account) {
            state.mark_visit_confirmed(Instant::now());
        }
        client.perform(MinecraftAction::ClickSlot(slot)).await?;
        self.clear_active_window_cache(account)?;
        Ok(true)
    }

    pub(in crate::live_runtime) fn handle_island_scoreboard(
        &mut self,
        account: &AccountId,
        lines: &[String],
    ) {
        let became_ready = if let Some(state) = self.island_states.get_mut(account) {
            let target_ready = state.record_scoreboard(lines);
            if state.use_cookie && target_ready {
                state.mark_ready()
            } else {
                false
            }
        } else {
            false
        };
        if became_ready {
            self.queue_startup_reconcile(account);
        }
    }

    pub(in crate::live_runtime) fn handle_island_chat(
        &mut self,
        account: &AccountId,
        text: &str,
    ) -> Result<Option<String>> {
        let Some(state) = self.island_states.get_mut(account) else {
            return Ok(None);
        };
        let now = Instant::now();
        let awaiting_locraw = state.awaiting_locraw;
        let ready = state.ready;
        if is_bad_modification_message(text) {
            tracing::warn!(
                account = %account,
                excerpt = %startup_chat_excerpt(text),
                "startup movement paused after unsafe-modification chat message"
            );
            state.mark_bad_modification(now);
            return Ok(None);
        }
        if text
            .to_ascii_lowercase()
            .contains("a kick occurred in your connection")
        {
            tracing::warn!(
                account = %account,
                excerpt = %startup_chat_excerpt(text),
                "startup movement saw connection kick chat message"
            );
            state.mark_disconnected();
            return Ok(None);
        }
        if text.contains("Welcome to Hypixel SkyBlock!") {
            tracing::info!(
                account = %account,
                use_cookie = state.use_cookie,
                "startup movement saw SkyBlock welcome chat message"
            );
            if state.use_cookie {
                state.schedule_locraw(now);
            } else {
                state.mark_moving();
                return Ok(Some("/hub".to_string()));
            }
        }

        let locraw = saf_core::island::parse_locraw_message(&Value::String(text.to_string()));
        let Some(locraw) = locraw else {
            let public_noise = startup_chat_is_public_noise(text);
            if !ready && !public_noise && (startup_chat_is_diagnostic(text) || awaiting_locraw) {
                if startup_chat_reports_skyblock_unavailable(text) {
                    state.mark_movement_backoff(now, SKYBLOCK_UNAVAILABLE_BACKOFF);
                    tracing::info!(
                        account = %account,
                        backoff_ms = SKYBLOCK_UNAVAILABLE_BACKOFF.as_millis(),
                        excerpt = %startup_chat_excerpt(text),
                        "startup movement is backing off because SkyBlock is unavailable"
                    );
                    return Ok(None);
                }
                tracing::info!(
                    account = %account,
                    awaiting_locraw,
                    excerpt = %startup_chat_excerpt(text),
                    "startup movement received non-locraw chat while account is not market-ready"
                );
            }
            return Ok(None);
        };
        state.finish_locraw();
        let command =
            saf_core::island::get_locraw_move(&locraw, &state.base_message, state.use_cookie);
        tracing::info!(
            account = %account,
            server = locraw.server.as_deref().unwrap_or(""),
            lobbyname = locraw.lobbyname.as_deref().unwrap_or(""),
            map = locraw.map.as_deref().unwrap_or(""),
            base_message = %state.base_message,
            use_cookie = state.use_cookie,
            movement_command = command.unwrap_or("ready"),
            "startup movement parsed locraw"
        );
        if let Some(command) = command {
            if state.bad_modification_backoff_active(now) {
                return Ok(None);
            }
            state.mark_moving();
            if command != "/skyblock" {
                state.schedule_locraw(now);
            }
            return Ok(Some(command.to_string()));
        }
        if state.should_visit_friend()
            && let Some(command) = state.mark_visit_pending()
        {
            return Ok(Some(command));
        }
        let became_ready = state.mark_ready();
        if became_ready {
            self.queue_startup_reconcile(account);
        }
        Ok(None)
    }
}

fn startup_chat_is_diagnostic(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    text.trim_start().starts_with('{')
        || startup_chat_reports_skyblock_unavailable(text)
        || lower.contains("status.hypixel.net")
        || lower.contains("limbo")
        || lower.contains("unknown command")
        || lower.contains("can't use")
        || lower.contains("cannot use")
        || lower.contains("must be")
        || lower.contains("not currently")
        || lower.contains("already")
}

fn startup_chat_reports_skyblock_unavailable(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("skyblock is currently")
        || lower.contains("skyblock will be undergoing maintenance")
        || lower.contains("skyblock will be unavailable")
}

fn startup_chat_is_public_noise(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    let ranked_chat = text.trim_start().starts_with('[')
        && text
            .split_once(':')
            .is_some_and(|(prefix, _)| prefix.contains(']'));
    let player_chat = text.split_once(':').is_some_and(|(prefix, _)| {
        let name = prefix.trim();
        (3..=16).contains(&name.len())
            && name
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
    });
    ranked_chat
        || player_chat
        || lower.contains("joined the lobby")
        || lower.contains("left the lobby")
}

fn startup_chat_excerpt(text: &str) -> String {
    const MAX_CHARS: usize = 240;
    let cleaned = text.split_whitespace().collect::<Vec<_>>().join(" ");
    cleaned.chars().take(MAX_CHARS).collect()
}

#[cfg(test)]
mod tests {
    use super::{startup_chat_is_diagnostic, startup_chat_is_public_noise};

    #[test]
    fn startup_chat_diagnostics_keep_maintenance_but_drop_lobby_noise() {
        assert!(startup_chat_is_diagnostic(
            "SkyBlock is currently undergoing maintenance, find out more at https://status.hypixel.net",
        ));
        assert!(!startup_chat_is_diagnostic(
            "[VIP] Someone: skyblock still has lag"
        ));
        assert!(startup_chat_is_diagnostic(
            "[VIP] Someone: https://status.hypixel.net"
        ));
        assert!(startup_chat_is_public_noise(
            ">>> [MVP++] Example joined the lobby! <<<"
        ));
        assert!(startup_chat_is_public_noise(
            "[VIP] Someone: https://status.hypixel.net"
        ));
        assert!(startup_chat_is_public_noise(
            "mxoi: SkyBlock will be undergoing maintenance"
        ));
        assert!(!startup_chat_is_public_noise(
            "SkyBlock is currently undergoing maintenance, find out more at https://status.hypixel.net"
        ));
    }
}
