use super::{azalea_crate, metadata::azalea_item_metadata};
use azalea_inventory::ItemStack as AzaleaItemStack;
use saf_core::gui::{WindowSlot, WindowSnapshot};
use saf_core::ports::MinecraftEvent;
use std::collections::BTreeMap;

#[derive(Clone, Debug, Default)]
pub(super) struct AzaleaWindowTracker {
    open_window: Option<TrackedAzaleaWindow>,
    last_emitted_window: Option<WindowSnapshot>,
    scoreboard: TrackedScoreboard,
}

impl AzaleaWindowTracker {
    pub(super) fn observe_event(&mut self, event: azalea_crate::Event) -> Option<MinecraftEvent> {
        match event {
            azalea_crate::Event::Spawn => Some(MinecraftEvent::Ready {
                reason: "spawn".to_string(),
            }),
            azalea_crate::Event::Chat(packet) => Some(MinecraftEvent::ChatMessage {
                text: packet.message().to_string(),
            }),
            azalea_crate::Event::Disconnect(reason) => {
                self.open_window = None;
                self.last_emitted_window = None;
                Some(map_disconnect_reason(
                    reason.map(|reason| reason.to_string()),
                ))
            }
            azalea_crate::Event::ConnectionFailed(error) => {
                self.open_window = None;
                self.last_emitted_window = None;
                Some(map_disconnect_reason(Some(error.to_string())))
            }
            azalea_crate::Event::Packet(packet) => self.observe_packet(packet.as_ref()),
            _ => None,
        }
    }

    pub(super) fn observe_packet(
        &mut self,
        packet: &azalea_crate::protocol::packets::game::ClientboundGamePacket,
    ) -> Option<MinecraftEvent> {
        match packet {
            azalea_crate::protocol::packets::game::ClientboundGamePacket::OpenScreen(packet) => {
                self.open_window = Some(TrackedAzaleaWindow {
                    container_id: packet.container_id,
                    title: packet.title.to_string(),
                    slots: Vec::new(),
                });
                None
            }
            azalea_crate::protocol::packets::game::ClientboundGamePacket::ContainerSetContent(
                packet,
            ) => {
                let snapshot = {
                    let window = self.open_window.as_mut()?;
                    if packet.container_id != window.container_id {
                        return None;
                    }
                    window.slots = packet.items.clone();
                    window.snapshot()
                };
                self.emit_window_snapshot(snapshot)
            }
            azalea_crate::protocol::packets::game::ClientboundGamePacket::ContainerSetSlot(
                packet,
            ) => {
                let snapshot = {
                    let window = self.open_window.as_mut()?;
                    if packet.container_id != window.container_id {
                        return None;
                    }
                    let slot = usize::from(packet.slot);
                    if slot >= window.slots.len() {
                        window.slots.resize(slot + 1, AzaleaItemStack::Empty);
                    }
                    window.slots[slot] = packet.item_stack.clone();
                    window.snapshot()
                };
                self.emit_window_snapshot(snapshot)
            }
            azalea_crate::protocol::packets::game::ClientboundGamePacket::ContainerClose(
                packet,
            ) => self.close_window(packet.container_id),
            azalea_crate::protocol::packets::game::ClientboundGamePacket::SetScore(packet) => {
                self.scoreboard.observe_score_packet(packet)
            }
            azalea_crate::protocol::packets::game::ClientboundGamePacket::SetPlayerTeam(packet) => {
                self.scoreboard.observe_team_packet(packet)
            }
            _ => map_azalea_packet(packet),
        }
    }

    fn close_window(&mut self, container_id: i32) -> Option<MinecraftEvent> {
        if !self
            .open_window
            .as_ref()
            .is_some_and(|window| window.container_id == container_id)
        {
            return None;
        }
        self.open_window = None;
        self.last_emitted_window = None;
        Some(MinecraftEvent::WindowClosed)
    }

    pub(super) fn observe_current_window_snapshot(
        &mut self,
        snapshot: Option<WindowSnapshot>,
    ) -> Option<MinecraftEvent> {
        let snapshot = snapshot.filter(is_gui_window_snapshot);
        match (self.last_emitted_window.as_ref(), snapshot) {
            (Some(_), None) => {
                self.open_window = None;
                self.last_emitted_window = None;
                Some(MinecraftEvent::WindowClosed)
            }
            (Some(previous), Some(current)) if previous == &current => None,
            (_, Some(current)) => self.emit_window_snapshot(current),
            (None, None) => None,
        }
    }

    fn emit_window_snapshot(&mut self, snapshot: WindowSnapshot) -> Option<MinecraftEvent> {
        if !is_gui_window_snapshot(&snapshot) {
            return None;
        }
        self.last_emitted_window = Some(snapshot.clone());
        Some(MinecraftEvent::WindowOpen(snapshot))
    }
}

fn is_gui_window_snapshot(snapshot: &WindowSnapshot) -> bool {
    let title = snapshot.title.trim();
    !title.is_empty() && !title.eq_ignore_ascii_case("inventory")
}

#[derive(Clone, Debug, Default)]
struct TrackedScoreboard {
    scores: BTreeMap<String, TrackedScoreboardEntry>,
    teams: BTreeMap<String, TrackedScoreboardTeam>,
}

impl TrackedScoreboard {
    fn observe_score_packet(
        &mut self,
        packet: &azalea_crate::protocol::packets::game::ClientboundSetScore,
    ) -> Option<MinecraftEvent> {
        self.scores.insert(
            packet.owner.clone(),
            TrackedScoreboardEntry {
                score: packet.score,
                display: packet.display.as_ref().map(ToString::to_string),
            },
        );
        self.event()
    }

    fn observe_team_packet(
        &mut self,
        packet: &azalea_crate::protocol::packets::game::ClientboundSetPlayerTeam,
    ) -> Option<MinecraftEvent> {
        use azalea_crate::protocol::packets::game::c_set_player_team::Method;

        match &packet.method {
            Method::Add((parameters, players)) => {
                self.teams.insert(
                    packet.name.clone(),
                    TrackedScoreboardTeam::from_parameters(parameters, players),
                );
            }
            Method::Remove => {
                self.teams.remove(&packet.name);
            }
            Method::Change(parameters) => {
                self.teams
                    .entry(packet.name.clone())
                    .or_default()
                    .update_parameters(parameters);
            }
            Method::Join(players) => {
                self.teams
                    .entry(packet.name.clone())
                    .or_default()
                    .join_players(players);
            }
            Method::Leave(players) => {
                if let Some(team) = self.teams.get_mut(&packet.name) {
                    team.leave_players(players);
                    if team.is_empty() {
                        self.teams.remove(&packet.name);
                    }
                }
            }
        }

        self.event()
    }

    fn event(&self) -> Option<MinecraftEvent> {
        let lines = self.render_lines();
        (!lines.is_empty()).then_some(MinecraftEvent::Scoreboard { lines })
    }

    fn render_lines(&self) -> Vec<String> {
        if self.scores.is_empty() {
            return self
                .teams
                .values()
                .flat_map(TrackedScoreboardTeam::render_lines)
                .filter(|line| !line.trim().is_empty())
                .collect();
        }

        let mut scored_lines = self
            .scores
            .iter()
            .map(|(owner, entry)| {
                (
                    std::cmp::Reverse(entry.score),
                    owner,
                    self.render_score_owner(owner, entry),
                )
            })
            .filter(|(_, _, line)| !line.trim().is_empty())
            .collect::<Vec<_>>();
        scored_lines.sort_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(right.1)));
        scored_lines.into_iter().map(|(_, _, line)| line).collect()
    }

    fn render_score_owner(&self, owner: &str, entry: &TrackedScoreboardEntry) -> String {
        self.teams
            .values()
            .find(|team| team.has_player(owner))
            .map(|team| team.render_player(owner))
            .or_else(|| entry.display.clone())
            .unwrap_or_else(|| owner.to_string())
    }
}

#[derive(Clone, Debug)]
struct TrackedScoreboardEntry {
    score: u32,
    display: Option<String>,
}

#[derive(Clone, Debug, Default)]
struct TrackedScoreboardTeam {
    prefix: String,
    suffix: String,
    players: Vec<String>,
}

impl TrackedScoreboardTeam {
    fn from_parameters(
        parameters: &azalea_crate::protocol::packets::game::c_set_player_team::Parameters,
        players: &[String],
    ) -> Self {
        let mut team = Self::default();
        team.update_parameters(parameters);
        team.join_players(players);
        team
    }

    fn update_parameters(
        &mut self,
        parameters: &azalea_crate::protocol::packets::game::c_set_player_team::Parameters,
    ) {
        self.prefix = parameters.player_prefix.to_string();
        self.suffix = parameters.player_suffix.to_string();
    }

    fn join_players(&mut self, players: &[String]) {
        for player in players {
            if !self.players.iter().any(|known| known == player) {
                self.players.push(player.clone());
            }
        }
    }

    fn leave_players(&mut self, players: &[String]) {
        self.players
            .retain(|known| !players.iter().any(|player| player == known));
    }

    fn has_player(&self, player: &str) -> bool {
        self.players.iter().any(|known| known == player)
    }

    fn is_empty(&self) -> bool {
        self.players.is_empty()
    }

    fn render_lines(&self) -> Vec<String> {
        if self.players.is_empty() {
            return vec![format!("{}{}", self.prefix, self.suffix)];
        }
        self.players
            .iter()
            .map(|player| self.render_player(player))
            .collect()
    }

    fn render_player(&self, player: &str) -> String {
        format!("{}{}{}", self.prefix, player, self.suffix)
    }
}

#[derive(Clone, Debug)]
struct TrackedAzaleaWindow {
    container_id: i32,
    title: String,
    slots: Vec<AzaleaItemStack>,
}

impl TrackedAzaleaWindow {
    fn snapshot(&self) -> WindowSnapshot {
        WindowSnapshot {
            title: self.title.clone(),
            slots: self
                .slots
                .iter()
                .enumerate()
                .filter(|(_, item)| item.is_present())
                .map(|(slot, item)| {
                    let metadata = azalea_item_metadata(item);
                    WindowSlot {
                        slot,
                        name: metadata.item_name,
                        display_name: metadata.display_name,
                        lore: metadata.lore,
                        item_uuid: metadata.item_uuid,
                    }
                })
                .collect(),
        }
    }
}
#[cfg(test)]
pub(super) fn map_azalea_event(event: azalea_crate::Event) -> Option<MinecraftEvent> {
    AzaleaWindowTracker::default().observe_event(event)
}

pub(super) fn map_disconnect_reason(reason: Option<String>) -> MinecraftEvent {
    let reason = reason.unwrap_or_else(|| "disconnected".to_string());
    if is_server_kick_reason(&reason) {
        MinecraftEvent::Kicked { reason }
    } else {
        MinecraftEvent::Disconnected { reason }
    }
}

fn is_server_kick_reason(reason: &str) -> bool {
    let lower = reason.to_ascii_lowercase();
    [
        "kick",
        "ban",
        "not authenticated",
        "already logged",
        "already connected",
        "logged in from another location",
        "outdated",
        "whitelist",
        "blacklist",
        "flying is not enabled",
        "badly behaving modifications",
        "commands too fast",
    ]
    .iter()
    .any(|pattern| lower.contains(pattern))
}

fn map_azalea_packet(
    packet: &azalea_crate::protocol::packets::game::ClientboundGamePacket,
) -> Option<MinecraftEvent> {
    match packet {
        azalea_crate::protocol::packets::game::ClientboundGamePacket::SetScore(packet) => {
            let mut lines = Vec::new();
            if let Some(display) = &packet.display {
                lines.push(display.to_string());
            }
            if !packet.owner.trim().is_empty() {
                lines.push(packet.owner.clone());
            }
            (!lines.is_empty()).then_some(MinecraftEvent::Scoreboard { lines })
        }
        _ => None,
    }
}
