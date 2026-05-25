use super::helpers::sanitize_inline;
use serde_json::Value;

pub(super) fn format_blacklist_snapshot(summary: &str) -> Option<String> {
    let snapshot: Value = serde_json::from_str(summary).ok()?;
    let mut lines = vec!["Live Blacklist Rules".to_string()];
    append_blacklist_section(&mut lines, "Do Not Buy", snapshot.get("doNotBuy"));
    append_blacklist_section(&mut lines, "Do Not Relist", snapshot.get("doNotRelist"));
    Some(lines.join("\n"))
}

fn append_blacklist_section(lines: &mut Vec<String>, title: &str, section: Option<&Value>) {
    lines.push(format!("{title}:"));
    let Some(section) = section else {
        lines.push("- none".to_string());
        return;
    };

    let mut wrote_rule = false;
    for (label, key) in [
        ("Tags", "tags"),
        ("Names", "names"),
        ("Enchantments", "enchantments"),
        ("Item + Enchantment", "itemEnchantments"),
    ] {
        let entries = section
            .get(key)
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .map(format_blacklist_entry)
            .collect::<Vec<_>>();
        if entries.is_empty() {
            continue;
        }
        wrote_rule = true;
        lines.push(format!("- {label}: {}", entries.join(", ")));
    }

    if !wrote_rule {
        lines.push("- none".to_string());
    }
}

fn format_blacklist_entry(entry: &Value) -> String {
    match entry {
        Value::String(value) => format!("`{}`", sanitize_inline(value)),
        Value::Number(number) => format!("`{number}`"),
        Value::Bool(value) => format!("`{value}`"),
        Value::Object(object) => {
            if let Some(value) = object.get("value") {
                let mut rendered = format_blacklist_entry(value);
                if let Some(expires_at) = object.get("expiresAt").and_then(Value::as_str) {
                    rendered.push_str(" until ");
                    rendered.push_str(&sanitize_inline(expires_at));
                }
                return rendered;
            }

            let mut parts = Vec::new();
            if let Some(tag) = object.get("tag").and_then(Value::as_str) {
                parts.push(tag.to_string());
            }
            if let Some(name) = object.get("name").and_then(Value::as_str) {
                parts.push(name.to_string());
            }
            if let Some(enchantment) = object.get("enchantment").and_then(Value::as_str) {
                let level = object
                    .get("level")
                    .and_then(Value::as_u64)
                    .map(|level| format!(" {level}"))
                    .unwrap_or_default();
                parts.push(format!("{enchantment}{level}"));
            }
            let mut rendered = if parts.is_empty() {
                "`rule`".to_string()
            } else {
                format!("`{}`", sanitize_inline(&parts.join(" + ")))
            };
            if let Some(expires_at) = object.get("expiresAt").and_then(Value::as_str) {
                rendered.push_str(" until ");
                rendered.push_str(&sanitize_inline(expires_at));
            }
            rendered
        }
        _ => "`rule`".to_string(),
    }
}
