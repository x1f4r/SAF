use std::ops::Range;

pub(crate) fn patch_string_value(raw: &str, key: &str, value: &str) -> Option<String> {
    let range = find_json5_string_value(raw, key)?;
    let mut next = raw.to_string();
    next.replace_range(range, &serde_json::to_string(value).ok()?);
    Some(next)
}

fn find_json5_string_value(raw: &str, key: &str) -> Option<Range<usize>> {
    let after_key = find_json5_key_end(raw, key)?;
    let colon = raw[after_key..].find(':')? + after_key;
    let value_start = skip_json5_space_and_comments(raw, colon + 1)?;
    let quote = raw[value_start..].chars().next()?;
    if quote != '"' && quote != '\'' {
        return None;
    }

    let mut escaped = false;
    let mut cursor = value_start + quote.len_utf8();
    for ch in raw[cursor..].chars() {
        if escaped {
            escaped = false;
        } else if ch == '\\' {
            escaped = true;
        } else if ch == quote {
            return Some(value_start..cursor + ch.len_utf8());
        }
        cursor += ch.len_utf8();
    }
    None
}

fn find_json5_key_end(raw: &str, key: &str) -> Option<usize> {
    let mut index = 0usize;
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

        if ch == '"' || ch == '\'' {
            let (candidate, end) = read_json5_string(raw, index)?;
            if candidate == key && json5_key_is_followed_by_colon(raw, end) {
                return Some(end);
            }
            index = end;
            continue;
        }

        if is_json5_identifier(ch) {
            let start = index;
            index += ch.len_utf8();
            while index < raw.len() {
                let Some(next) = raw[index..].chars().next() else {
                    break;
                };
                if !is_json5_identifier(next) {
                    break;
                }
                index += next.len_utf8();
            }
            if &raw[start..index] == key && json5_key_is_followed_by_colon(raw, index) {
                return Some(index);
            }
            continue;
        }

        index += ch.len_utf8();
    }
    None
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

fn read_json5_string(raw: &str, start: usize) -> Option<(String, usize)> {
    let quote = raw[start..].chars().next()?;
    if quote != '"' && quote != '\'' {
        return None;
    }

    let mut value = String::new();
    let mut escaped = false;
    let mut cursor = start + quote.len_utf8();
    for ch in raw[cursor..].chars() {
        if escaped {
            value.push(ch);
            escaped = false;
        } else if ch == '\\' {
            escaped = true;
        } else if ch == quote {
            return Some((value, cursor + ch.len_utf8()));
        } else {
            value.push(ch);
        }
        cursor += ch.len_utf8();
    }
    None
}

fn json5_key_is_followed_by_colon(raw: &str, index: usize) -> bool {
    skip_json5_space_and_comments(raw, index)
        .and_then(|index| raw[index..].chars().next())
        .is_some_and(|ch| ch == ':')
}

fn is_json5_identifier(ch: char) -> bool {
    ch == '$' || ch == '_' || ch.is_ascii_alphanumeric()
}
