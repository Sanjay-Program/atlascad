//! Global asynchronous event bus and event notifications.

use serde::{Deserialize, Serialize};

/// High-level engineering event notifications.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CoreEvent {
    /// Document state modified.
    DocumentModified {
        document_id: String,
        revision: u64,
        operation: String,
    },
    /// A solver job was initiated.
    JobStarted { job_id: String, engine_id: String },
    /// A solver job completed successfully.
    JobCompleted {
        job_id: String,
        engine_id: String,
        elapsed_ms: u64,
    },
    /// A solver job failed or was cancelled.
    JobFailed {
        job_id: String,
        engine_id: String,
        error: String,
    },
    /// Engineering workspace switched.
    WorkspaceChanged { previous: String, current: String },
}

/// Simple in-memory event dispatcher recording recent notifications.
#[derive(Debug, Default, Clone)]
pub struct EventJournal {
    events: Vec<CoreEvent>,
    max_history: usize,
}

impl EventJournal {
    /// Create new event journal with bounded history length.
    pub fn new(max_history: usize) -> Self {
        Self {
            events: Vec::new(),
            max_history,
        }
    }

    /// Record an event in the journal.
    pub fn emit(&mut self, event: CoreEvent) {
        if self.events.len() >= self.max_history {
            self.events.remove(0);
        }
        self.events.push(event);
    }

    /// Retrieve recorded events.
    pub fn events(&self) -> &[CoreEvent] {
        &self.events
    }

    /// Clear all events.
    pub fn clear(&mut self) {
        self.events.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_journal_bounds() {
        let mut journal = EventJournal::new(2);
        journal.emit(CoreEvent::JobStarted {
            job_id: "j1".to_string(),
            engine_id: "ccx".to_string(),
        });
        journal.emit(CoreEvent::JobCompleted {
            job_id: "j1".to_string(),
            engine_id: "ccx".to_string(),
            elapsed_ms: 120,
        });
        journal.emit(CoreEvent::WorkspaceChanged {
            previous: "drafting".to_string(),
            current: "bim".to_string(),
        });

        assert_eq!(journal.events().len(), 2);
        assert_eq!(
            journal.events()[1],
            CoreEvent::WorkspaceChanged {
                previous: "drafting".to_string(),
                current: "bim".to_string(),
            }
        );
    }
}
