#[derive(Debug, thiserror::Error)]
pub enum CoflParseError {
    #[error("invalid cofl json: {0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Debug, thiserror::Error)]
pub enum CoflCommandError {
    #[error("missing cofl command")]
    MissingCommand,
    #[error("invalid cofl command json: {0}")]
    Json(#[from] serde_json::Error),
}
