use anyhow::{Context, Result};
use serde_json::Value;
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct CommandInboxCursor {
    path: PathBuf,
    offset: usize,
}

impl CommandInboxCursor {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            offset: 0,
        }
    }

    pub async fn new_at_end(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        let offset = match tokio::fs::read_to_string(&path).await {
            Ok(raw) => complete_inbox_prefix_end(&raw, 0),
            Err(_) => 0,
        };
        Self { path, offset }
    }

    pub async fn read_new(&mut self) -> Result<String> {
        let raw = match tokio::fs::read_to_string(&self.path).await {
            Ok(raw) => raw,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                self.offset = 0;
                return Ok(String::new());
            }
            Err(error) => {
                return Err(error).with_context(|| format!("reading {}", self.path.display()));
            }
        };

        if self.offset > raw.len() || !raw.is_char_boundary(self.offset) {
            self.offset = 0;
        }
        let read_end = complete_inbox_prefix_end(&raw, self.offset);
        let new = raw[self.offset..read_end].to_string();
        self.offset = read_end;
        Ok(new)
    }
}

fn complete_inbox_prefix_end(raw: &str, offset: usize) -> usize {
    let tail = &raw[offset..];
    if tail.is_empty() {
        return offset;
    }
    if let Some(newline) = tail.rfind('\n') {
        return offset + newline + 1;
    }
    if should_defer_incomplete_json_tail(tail) {
        offset
    } else {
        raw.len()
    }
}

fn should_defer_incomplete_json_tail(tail: &str) -> bool {
    let trimmed = tail.trim();
    if trimmed.is_empty() || !trimmed.starts_with('{') {
        return false;
    }
    serde_json::from_str::<Value>(trimmed).is_err_and(|error| error.is_eof())
}
