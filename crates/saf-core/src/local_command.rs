use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum LocalCommand {
    #[serde(rename = "terminal")]
    Terminal {
        #[serde(default, alias = "command")]
        line: String,
        #[serde(default, rename = "createdAt")]
        created_at: Option<u64>,
    },
    #[serde(rename = "transfer")]
    Transfer {
        from: String,
        to: String,
        #[serde(default = "default_transfer_amount")]
        amount: String,
        #[serde(default = "default_transfer_stop_source", rename = "stopSource")]
        stop_source: bool,
        #[serde(default, rename = "createdAt")]
        created_at: Option<u64>,
    },
}

impl LocalCommand {
    pub fn terminal_line(&self) -> Option<&str> {
        match self {
            Self::Terminal { line, .. } => Some(line.trim()).filter(|line| !line.is_empty()),
            Self::Transfer { .. } => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LocalCommandLine {
    Parsed(LocalCommand),
    Invalid {
        line: String,
        error: LocalCommandParseError,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum LocalCommandParseError {
    #[error("empty line")]
    Empty,
    #[error("invalid json: {0}")]
    InvalidJson(String),
}

pub fn parse_jsonl(raw: &str) -> Vec<LocalCommandLine> {
    raw.lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                return None;
            }
            Some(parse_line(trimmed))
        })
        .collect()
}

pub fn parse_line(line: &str) -> LocalCommandLine {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return LocalCommandLine::Invalid {
            line: line.to_string(),
            error: LocalCommandParseError::Empty,
        };
    }

    match serde_json::from_str::<LocalCommand>(trimmed) {
        Ok(command) => LocalCommandLine::Parsed(command),
        Err(error) => LocalCommandLine::Invalid {
            line: line.to_string(),
            error: LocalCommandParseError::InvalidJson(error.to_string()),
        },
    }
}

fn default_transfer_amount() -> String {
    "all".to_string()
}

fn default_transfer_stop_source() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_terminal_and_transfer_inbox_lines() {
        let lines = parse_jsonl(
            r#"
{"type":"terminal","line":"/cofl switchregion US","createdAt":1}
{"type":"transfer","from":"Main","to":"Alt","amount":"all","stopSource":true,"createdAt":2}
bad-json
"#,
        );

        assert_eq!(lines.len(), 3);
        assert_eq!(
            lines[0],
            LocalCommandLine::Parsed(LocalCommand::Terminal {
                line: "/cofl switchregion US".to_string(),
                created_at: Some(1)
            })
        );
        assert!(matches!(
            lines[1],
            LocalCommandLine::Parsed(LocalCommand::Transfer { .. })
        ));
        assert!(matches!(
            lines[2],
            LocalCommandLine::Invalid {
                error: LocalCommandParseError::InvalidJson(_),
                ..
            }
        ));
    }

    #[test]
    fn transfer_inbox_defaults_match_node_controller() {
        let LocalCommandLine::Parsed(LocalCommand::Transfer {
            amount,
            stop_source,
            ..
        }) = parse_line(r#"{"type":"transfer","from":"Main","to":"Alt"}"#)
        else {
            panic!("transfer line should parse");
        };

        assert_eq!(amount, "all");
        assert!(stop_source);
    }
}
