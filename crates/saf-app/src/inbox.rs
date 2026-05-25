mod records;
mod session;
mod stores;

pub use records::{
    AccountScheduleRecord, BlacklistRecord, CoflCommandRecord, InboxCommandResult,
    InboxProcessingReport, LifecycleAction, LifecycleRecord, MinecraftActionRecord, QueueRecord,
    SavedDataClearRecord, StatsRequestKind, StatsRequestRecord,
};
pub use session::RecordedRuntimeSession;
pub use stores::FileLogReader;

use saf_core::{LocalCommandLine, RuntimeSession, local_command::parse_jsonl};

pub async fn process_inbox(session: &RuntimeSession, raw: &str) -> Vec<InboxCommandResult> {
    let mut results = Vec::new();
    for (index, line) in parse_jsonl(raw).into_iter().enumerate() {
        let command_index = index + 1;
        match line {
            LocalCommandLine::Parsed(command) => {
                let result = session.process_local_command(command).await;
                match result {
                    Ok(outcome) => results.push(InboxCommandResult::Processed {
                        index: command_index,
                        outcome,
                    }),
                    Err(error) => results.push(InboxCommandResult::Failed {
                        index: command_index,
                        error: error.to_string(),
                    }),
                }
            }
            LocalCommandLine::Invalid { line, error } => {
                results.push(InboxCommandResult::Invalid {
                    index: command_index,
                    line,
                    error: error.to_string(),
                });
            }
        }
    }
    results
}

#[cfg(test)]
mod tests;
