use saf_core::ports::GuiSlotDiagnostics;

pub(super) fn format_gui_slot_diagnostics(diagnostics: &GuiSlotDiagnostics) -> String {
    let mut lines = vec![format!(
        "GUI Slots `{}`{}",
        diagnostics.account,
        diagnostics
            .target
            .as_deref()
            .map(|target| format!(" target `{target}`"))
            .unwrap_or_default()
    )];
    for window in diagnostics.windows.iter().take(4) {
        lines.push(format!(
            "Window: {} ({} slots)",
            window.title,
            window.slots.len()
        ));
        lines.extend(window.slots.iter().take(10).map(|slot| {
            let label = if slot.display_name.trim().is_empty() {
                slot.name.as_str()
            } else {
                slot.display_name.as_str()
            };
            format!(
                "- {} {}{}",
                slot.slot,
                label,
                slot.item_uuid
                    .as_deref()
                    .map(|uuid| format!(" `{uuid}`"))
                    .unwrap_or_default()
            )
        }));
    }
    lines.join("\n")
}
