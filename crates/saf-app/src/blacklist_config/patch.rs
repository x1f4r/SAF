use serde_json::Value;
use std::ops::Range;

pub(super) fn patch_config_array(
    raw: &str,
    section: &str,
    field: &str,
    entries: &[Value],
) -> Option<String> {
    let range = find_json5_array_range(raw, section, field)?;
    let mut next = raw.to_string();
    let replacement = serde_json::to_string_pretty(&Value::Array(entries.to_vec())).ok()?;
    next.replace_range(range, &replacement);
    Some(next)
}

fn find_json5_array_range(raw: &str, section: &str, field: &str) -> Option<Range<usize>> {
    let section_object = find_json5_object_range(raw, section)?;
    let section_raw = &raw[section_object.clone()];
    let field_end = find_json5_key_end(section_raw, field)?;
    let colon = section_raw[field_end..].find(':')? + field_end;
    let value_start = skip_json5_space_and_comments(section_raw, colon + 1)?;
    if section_raw[value_start..].chars().next()? != '[' {
        return None;
    }
    let range = find_matching_delimiter(section_raw, value_start, '[', ']')?;
    Some(section_object.start + range.start..section_object.start + range.end)
}

fn find_json5_object_range(raw: &str, key: &str) -> Option<Range<usize>> {
    let key_end = find_json5_key_end(raw, key)?;
    let colon = raw[key_end..].find(':')? + key_end;
    let value_start = skip_json5_space_and_comments(raw, colon + 1)?;
    if raw[value_start..].chars().next()? != '{' {
        return None;
    }
    find_matching_delimiter(raw, value_start, '{', '}')
}

fn find_json5_key_end(raw: &str, key: &str) -> Option<usize> {
    raw.find(&format!("\"{key}\""))
        .map(|start| start + key.len() + 2)
        .or_else(|| {
            raw.find(&format!("'{key}'"))
                .map(|start| start + key.len() + 2)
        })
        .or_else(|| {
            raw.match_indices(key)
                .find(|(start, _)| {
                    let before = raw[..*start].chars().next_back();
                    let after = raw[*start + key.len()..].chars().next();
                    !before.is_some_and(is_json5_identifier)
                        && !after.is_some_and(is_json5_identifier)
                })
                .map(|(start, _)| start + key.len())
        })
}

fn skip_json5_space_and_comments(raw: &str, mut index: usize) -> Option<usize> {
    while index < raw.len() {
        let rest = &raw[index..];
        if rest.starts_with("//") {
            index += rest.find('\n').unwrap_or(rest.len());
            continue;
        }
        if rest.starts_with("/*") {
            index += rest.find("*/")? + 2;
            continue;
        }
        let ch = rest.chars().next()?;
        if ch.is_whitespace() {
            index += ch.len_utf8();
            continue;
        }
        break;
    }
    Some(index)
}

fn find_matching_delimiter(
    raw: &str,
    start: usize,
    opener: char,
    closer: char,
) -> Option<Range<usize>> {
    let mut index = start;
    let mut depth = 0_i32;
    let mut quote = None;
    let mut escaped = false;
    let mut line_comment = false;
    let mut block_comment = false;
    while index < raw.len() {
        let rest = &raw[index..];
        let ch = rest.chars().next()?;
        let len = ch.len_utf8();
        if line_comment {
            if ch == '\n' {
                line_comment = false;
            }
            index += len;
            continue;
        }
        if block_comment {
            if rest.starts_with("*/") {
                block_comment = false;
                index += 2;
            } else {
                index += len;
            }
            continue;
        }
        if let Some(active_quote) = quote {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == active_quote {
                quote = None;
            }
            index += len;
            continue;
        }
        if rest.starts_with("//") {
            line_comment = true;
            index += 2;
            continue;
        }
        if rest.starts_with("/*") {
            block_comment = true;
            index += 2;
            continue;
        }
        if ch == '"' || ch == '\'' {
            quote = Some(ch);
            index += len;
            continue;
        }
        if ch == opener {
            depth += 1;
        } else if ch == closer {
            depth -= 1;
            if depth == 0 {
                return Some(start..index + len);
            }
        }
        index += len;
    }
    None
}

fn is_json5_identifier(ch: char) -> bool {
    ch == '$' || ch == '_' || ch.is_ascii_alphanumeric()
}
