use serde::{Deserialize, Serialize};

use hera_types::CanonicalEvent;

/// Provides the future extension point for sanctions and exposure scoring while
/// keeping Stage 1 behavior explicit and deterministic.
pub struct RiskEvaluator;

/// Expresses how urgent a downstream consumer should consider a risk flag so the
/// schema can support future policy workflows without redesign.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Low,
    Medium,
    High,
    Critical,
}

/// Carries one compliance risk annotation that can be attached to a canonical
/// event once screening and scoring logic are enabled.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct RiskFlag {
    pub flag_type: String,
    pub severity: Severity,
    pub description: String,
}

impl RiskEvaluator {
    /// Risk evaluation is stubbed in Stage 1. Stage 4 will add OFAC screening
    /// and exposure scoring. The schema supports `risk_flags` from day one so
    /// reports don't need redesigning later.
    pub fn evaluate(&self, _event: &CanonicalEvent) -> Vec<RiskFlag> {
        Vec::new()
    }
}
