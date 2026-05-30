use saf_core::ports::NotificationKind;

/// Blurple used for neutral, dashboard, and informational cards.
pub const COLOR_BLURPLE: u32 = 0x5865F2;

/// Resolve the embed accent colour for a notification kind. The mapping is
/// shared by both output paths (webhook embeds and gateway replies) so the
/// same event always reads with the same colour.
pub fn kind_color(kind: NotificationKind) -> u32 {
    match kind {
        NotificationKind::FlipFound => 0x3BA7FF,
        NotificationKind::Bought => 0x2ECC71,
        NotificationKind::Sold => 0xF1C40F,
        NotificationKind::Listed | NotificationKind::Relisted => 0x1ABC9C,
        NotificationKind::Blocked => 0xE67E22,
        NotificationKind::Error | NotificationKind::LoginRequired | NotificationKind::Stopped => {
            0xE74C3C
        }
        NotificationKind::Started | NotificationKind::Info => COLOR_BLURPLE,
    }
}

/// Resolve the leading icon for a notification kind, used to prefix card
/// titles so the event type is obvious at a glance.
pub fn kind_icon(kind: NotificationKind) -> &'static str {
    match kind {
        NotificationKind::FlipFound => "\u{1F50D}",
        NotificationKind::Bought => "\u{1F4B0}",
        NotificationKind::Sold => "\u{1F4B0}",
        NotificationKind::Listed | NotificationKind::Relisted => "\u{1F3F7}\u{FE0F}",
        NotificationKind::Blocked => "\u{26A0}\u{FE0F}",
        NotificationKind::Error => "\u{26A0}\u{FE0F}",
        NotificationKind::LoginRequired => "\u{1F510}",
        NotificationKind::Stopped => "\u{1F6D1}",
        NotificationKind::Started => "\u{25B6}\u{FE0F}",
        NotificationKind::Info => "\u{1F39B}\u{FE0F}",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palette_matches_approved_mock_colors() {
        assert_eq!(kind_color(NotificationKind::FlipFound), 0x3BA7FF);
        assert_eq!(kind_color(NotificationKind::Bought), 0x2ECC71);
        assert_eq!(kind_color(NotificationKind::Sold), 0xF1C40F);
        assert_eq!(kind_color(NotificationKind::Listed), 0x1ABC9C);
        assert_eq!(kind_color(NotificationKind::Relisted), 0x1ABC9C);
        assert_eq!(kind_color(NotificationKind::Blocked), 0xE67E22);
        assert_eq!(kind_color(NotificationKind::Error), 0xE74C3C);
        assert_eq!(kind_color(NotificationKind::LoginRequired), 0xE74C3C);
        assert_eq!(kind_color(NotificationKind::Stopped), 0xE74C3C);
        assert_eq!(kind_color(NotificationKind::Started), COLOR_BLURPLE);
        assert_eq!(kind_color(NotificationKind::Info), COLOR_BLURPLE);
    }

    #[test]
    fn every_kind_has_a_non_empty_icon() {
        for kind in [
            NotificationKind::FlipFound,
            NotificationKind::Bought,
            NotificationKind::Sold,
            NotificationKind::Listed,
            NotificationKind::Relisted,
            NotificationKind::Blocked,
            NotificationKind::Error,
            NotificationKind::LoginRequired,
            NotificationKind::Stopped,
            NotificationKind::Started,
            NotificationKind::Info,
        ] {
            assert!(!kind_icon(kind).is_empty(), "{kind:?}");
        }
    }
}
