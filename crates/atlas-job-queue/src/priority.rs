//! Priority classes for simulation workload scheduling.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobPriority {
    BatchLow = 0,
    InteractiveNormal = 1,
    HighPriority = 2,
    CriticalHpc = 3,
}

impl JobPriority {
    pub const fn label(&self) -> &'static str {
        match self {
            Self::BatchLow => "Low (Batch)",
            Self::InteractiveNormal => "Normal (Interactive)",
            Self::HighPriority => "High (Design Review)",
            Self::CriticalHpc => "Critical (Enterprise HPC)",
        }
    }
}
