//! Server-side subscription entitlements.
//!
//! Quotas are evaluated here, not by hiding UI. A frontend that omits a button is not access
//! control. This crate does not talk to a payment provider; it applies a policy to an account
//! state and to idempotent billing events so the later Stripe (or other) adapter has a contract
//! to implement against.
//!
//! Prices in the bundled catalogue are **targets**, not live charges. GST and other taxes are
//! collected separately from net revenue and are not modelled as product income.

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Target list prices in INR, excluding tax. Hypothesis, not a live price book.
pub const CREATOR_INR_PER_MONTH: u32 = 399;
pub const PROFESSIONAL_INR_PER_MONTH: u32 = 999;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tier {
    Student,
    Creator,
    Professional,
    Enterprise,
}

impl Tier {
    pub const fn label(self) -> &'static str {
        match self {
            Tier::Student => "student",
            Tier::Creator => "creator",
            Tier::Professional => "professional",
            Tier::Enterprise => "enterprise",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Period {
    Day,
    Month,
    Year,
    Lifetime,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Overage {
    Deny,
    GraceThenDeny,
    MeteredCharge,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PaymentStatus {
    Active,
    PastDue,
    Cancelled,
    Refunded,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Entitlement {
    pub feature_id: String,
    pub tiers: Vec<Tier>,
    /// `None` means the feature is included without a numeric cap on that period.
    pub limit: Option<u64>,
    pub period: Period,
    pub overage: Overage,
    pub grace_units: u64,
    pub upgrade_path: Option<Tier>,
    pub audit: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Catalog {
    pub schema_version: u32,
    pub currency: String,
    pub tax_note: String,
    pub entitlements: Vec<Entitlement>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Account {
    pub id: String,
    pub tier: Tier,
    pub payment: PaymentStatus,
    pub usage: BTreeMap<String, u64>,
    pub processed_events: BTreeSet<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Decision {
    Allow,
    Deny,
    Grace,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EntitlementError {
    #[error("unknown feature `{0}`")]
    UnknownFeature(String),
    #[error("feature `{feature}` is not included in the {tier} tier")]
    NotInTier { feature: String, tier: &'static str },
    #[error("account `{0}` is not in an active payment state")]
    Inactive(String),
    #[error("duplicate billing event `{0}` was ignored")]
    DuplicateEvent(String),
    #[error("{0}")]
    Invalid(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BillingEvent {
    PaymentSucceeded { event_id: String },
    PaymentFailed { event_id: String },
    Cancel { event_id: String },
    Refund { event_id: String },
    Upgrade { event_id: String, to: Tier },
    Downgrade { event_id: String, to: Tier },
    Renew { event_id: String },
}

impl Account {
    pub fn new(id: impl Into<String>, tier: Tier) -> Self {
        Account {
            id: id.into(),
            tier,
            payment: PaymentStatus::Active,
            usage: BTreeMap::new(),
            processed_events: BTreeSet::new(),
        }
    }

    pub fn consume(&mut self, feature_id: &str, units: u64) {
        let used = self.usage.entry(feature_id.to_string()).or_insert(0);
        *used = used.saturating_add(units);
    }
}

impl Catalog {
    /// The initial product catalogue. Limits are cost hypotheses, not measured customer usage.
    pub fn initial() -> Self {
        Catalog {
            schema_version: 1,
            currency: "INR".into(),
            tax_note: "List prices are exclusive of GST and other applicable taxes. Tax treatment must be confirmed with a qualified professional before charging customers.".into(),
            entitlements: vec![
                Entitlement {
                    feature_id: "cad.local.edit".into(),
                    tiers: vec![
                        Tier::Student,
                        Tier::Creator,
                        Tier::Professional,
                        Tier::Enterprise,
                    ],
                    limit: None,
                    period: Period::Lifetime,
                    overage: Overage::Deny,
                    grace_units: 0,
                    upgrade_path: None,
                    audit: true,
                },
                Entitlement {
                    feature_id: "cad.commercial.use".into(),
                    tiers: vec![Tier::Creator, Tier::Professional, Tier::Enterprise],
                    limit: None,
                    period: Period::Lifetime,
                    overage: Overage::Deny,
                    grace_units: 0,
                    upgrade_path: Some(Tier::Creator),
                    audit: true,
                },
                Entitlement {
                    feature_id: "storage.cloud.gb".into(),
                    tiers: vec![
                        Tier::Student,
                        Tier::Creator,
                        Tier::Professional,
                        Tier::Enterprise,
                    ],
                    limit: Some(1),
                    period: Period::Month,
                    overage: Overage::Deny,
                    grace_units: 0,
                    upgrade_path: Some(Tier::Creator),
                    audit: true,
                },
                Entitlement {
                    feature_id: "jobs.geometry".into(),
                    tiers: vec![
                        Tier::Student,
                        Tier::Creator,
                        Tier::Professional,
                        Tier::Enterprise,
                    ],
                    limit: Some(50),
                    period: Period::Month,
                    overage: Overage::GraceThenDeny,
                    grace_units: 5,
                    upgrade_path: Some(Tier::Professional),
                    audit: true,
                },
                Entitlement {
                    feature_id: "ai.assist".into(),
                    tiers: vec![Tier::Creator, Tier::Professional, Tier::Enterprise],
                    limit: Some(20),
                    period: Period::Month,
                    overage: Overage::Deny,
                    grace_units: 0,
                    upgrade_path: Some(Tier::Professional),
                    audit: true,
                },
                Entitlement {
                    feature_id: "collab.private".into(),
                    tiers: vec![Tier::Professional, Tier::Enterprise],
                    limit: Some(5),
                    period: Period::Lifetime,
                    overage: Overage::Deny,
                    grace_units: 0,
                    upgrade_path: Some(Tier::Professional),
                    audit: true,
                },
                Entitlement {
                    feature_id: "admin.org".into(),
                    tiers: vec![Tier::Enterprise],
                    limit: None,
                    period: Period::Lifetime,
                    overage: Overage::Deny,
                    grace_units: 0,
                    upgrade_path: Some(Tier::Enterprise),
                    audit: true,
                },
            ],
        }
    }

    pub fn with_tier_limits(mut self) -> Self {
        for entitlement in &mut self.entitlements {
            if entitlement.feature_id == "storage.cloud.gb" {
                // Filled per-account at evaluate time via `limit_for`.
            }
        }
        self
    }

    pub fn get(&self, feature_id: &str) -> Result<&Entitlement, EntitlementError> {
        self.entitlements
            .iter()
            .find(|entitlement| entitlement.feature_id == feature_id)
            .ok_or_else(|| EntitlementError::UnknownFeature(feature_id.to_string()))
    }

    /// Storage included per tier. These are cost-control hypotheses.
    pub fn storage_gb(tier: Tier) -> u64 {
        match tier {
            Tier::Student => 1,
            Tier::Creator => 5,
            Tier::Professional => 50,
            Tier::Enterprise => 500,
        }
    }

    pub fn geometry_jobs(tier: Tier) -> u64 {
        match tier {
            Tier::Student => 50,
            Tier::Creator => 200,
            Tier::Professional => 2_000,
            Tier::Enterprise => 20_000,
        }
    }

    pub fn ai_requests(tier: Tier) -> u64 {
        match tier {
            Tier::Student => 0,
            Tier::Creator => 20,
            Tier::Professional => 200,
            Tier::Enterprise => 2_000,
        }
    }

    fn limit_for(&self, entitlement: &Entitlement, tier: Tier) -> Option<u64> {
        match entitlement.feature_id.as_str() {
            "storage.cloud.gb" => Some(Self::storage_gb(tier)),
            "jobs.geometry" => Some(Self::geometry_jobs(tier)),
            "ai.assist" => Some(Self::ai_requests(tier)),
            _ => entitlement.limit,
        }
    }

    pub fn evaluate(
        &self,
        account: &Account,
        feature_id: &str,
        additional: u64,
    ) -> Result<Decision, EntitlementError> {
        if account.payment != PaymentStatus::Active && feature_id != "cad.local.edit" {
            return Err(EntitlementError::Inactive(account.id.clone()));
        }
        let entitlement = self.get(feature_id)?;
        if !entitlement.tiers.contains(&account.tier) {
            return Err(EntitlementError::NotInTier {
                feature: feature_id.to_string(),
                tier: account.tier.label(),
            });
        }
        let Some(limit) = self.limit_for(entitlement, account.tier) else {
            return Ok(Decision::Allow);
        };
        if limit == 0 {
            return Err(EntitlementError::NotInTier {
                feature: feature_id.to_string(),
                tier: account.tier.label(),
            });
        }
        let used = account.usage.get(feature_id).copied().unwrap_or(0);
        let projected = used.saturating_add(additional);
        if projected <= limit {
            return Ok(Decision::Allow);
        }
        match entitlement.overage {
            Overage::Deny | Overage::MeteredCharge => Ok(Decision::Deny),
            Overage::GraceThenDeny => {
                if projected <= limit.saturating_add(entitlement.grace_units) {
                    Ok(Decision::Grace)
                } else {
                    Ok(Decision::Deny)
                }
            }
        }
    }

    /// Apply a billing event. Duplicate `event_id` values are no-ops after the first success.
    pub fn apply_event(
        &self,
        account: &mut Account,
        event: BillingEvent,
    ) -> Result<(), EntitlementError> {
        let event_id = match &event {
            BillingEvent::PaymentSucceeded { event_id }
            | BillingEvent::PaymentFailed { event_id }
            | BillingEvent::Cancel { event_id }
            | BillingEvent::Refund { event_id }
            | BillingEvent::Upgrade { event_id, .. }
            | BillingEvent::Downgrade { event_id, .. }
            | BillingEvent::Renew { event_id } => event_id.clone(),
        };
        if !account.processed_events.insert(event_id.clone()) {
            return Err(EntitlementError::DuplicateEvent(event_id));
        }
        match event {
            BillingEvent::PaymentSucceeded { .. } | BillingEvent::Renew { .. } => {
                account.payment = PaymentStatus::Active;
            }
            BillingEvent::PaymentFailed { .. } => {
                account.payment = PaymentStatus::PastDue;
            }
            BillingEvent::Cancel { .. } => {
                account.payment = PaymentStatus::Cancelled;
                account.tier = Tier::Student;
            }
            BillingEvent::Refund { .. } => {
                account.payment = PaymentStatus::Refunded;
                account.tier = Tier::Student;
            }
            BillingEvent::Upgrade { to, .. } => {
                if to < account.tier {
                    return Err(EntitlementError::Invalid(
                        "upgrade target is below the current tier".into(),
                    ));
                }
                account.tier = to;
                account.payment = PaymentStatus::Active;
            }
            BillingEvent::Downgrade { to, .. } => {
                if to > account.tier {
                    return Err(EntitlementError::Invalid(
                        "downgrade target is above the current tier".into(),
                    ));
                }
                account.tier = to;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn catalog() -> Catalog {
        Catalog::initial().with_tier_limits()
    }

    #[test]
    fn student_may_edit_locally_but_not_commercially() -> TestResult {
        let catalog = catalog();
        let account = Account::new("s1", Tier::Student);
        assert_eq!(
            catalog.evaluate(&account, "cad.local.edit", 1)?,
            Decision::Allow
        );
        match catalog.evaluate(&account, "cad.commercial.use", 1) {
            Err(EntitlementError::NotInTier { .. }) => Ok(()),
            other => Err(format!("unexpected {other:?}").into()),
        }
    }

    #[test]
    fn creator_is_denied_after_storage_quota() -> TestResult {
        let catalog = catalog();
        let mut account = Account::new("c1", Tier::Creator);
        account.consume("storage.cloud.gb", 5);
        assert_eq!(
            catalog.evaluate(&account, "storage.cloud.gb", 1)?,
            Decision::Deny
        );
        Ok(())
    }

    #[test]
    fn geometry_jobs_enter_grace_then_deny() -> TestResult {
        let catalog = catalog();
        let mut account = Account::new("s2", Tier::Student);
        account.consume("jobs.geometry", 50);
        assert_eq!(
            catalog.evaluate(&account, "jobs.geometry", 1)?,
            Decision::Grace
        );
        account.consume("jobs.geometry", 5);
        assert_eq!(
            catalog.evaluate(&account, "jobs.geometry", 1)?,
            Decision::Deny
        );
        Ok(())
    }

    #[test]
    fn upgrade_unlocks_collaboration() -> TestResult {
        let catalog = catalog();
        let mut account = Account::new("c2", Tier::Creator);
        assert!(catalog.evaluate(&account, "collab.private", 1).is_err());
        catalog.apply_event(
            &mut account,
            BillingEvent::Upgrade {
                event_id: "evt-up-1".into(),
                to: Tier::Professional,
            },
        )?;
        assert_eq!(account.tier, Tier::Professional);
        assert_eq!(
            catalog.evaluate(&account, "collab.private", 1)?,
            Decision::Allow
        );
        Ok(())
    }

    #[test]
    fn duplicate_webhook_does_not_apply_twice() -> TestResult {
        let catalog = catalog();
        let mut account = Account::new("c3", Tier::Creator);
        catalog.apply_event(
            &mut account,
            BillingEvent::Upgrade {
                event_id: "evt-dup".into(),
                to: Tier::Professional,
            },
        )?;
        match catalog.apply_event(
            &mut account,
            BillingEvent::Upgrade {
                event_id: "evt-dup".into(),
                to: Tier::Enterprise,
            },
        ) {
            Err(EntitlementError::DuplicateEvent(id)) => {
                assert_eq!(id, "evt-dup");
                assert_eq!(account.tier, Tier::Professional);
                Ok(())
            }
            other => Err(format!("unexpected {other:?}").into()),
        }
    }

    #[test]
    fn failed_payment_blocks_cloud_features_but_local_edit_remains() -> TestResult {
        let catalog = catalog();
        let mut account = Account::new("c4", Tier::Creator);
        catalog.apply_event(
            &mut account,
            BillingEvent::PaymentFailed {
                event_id: "evt-fail".into(),
            },
        )?;
        assert!(catalog.evaluate(&account, "ai.assist", 1).is_err());
        assert_eq!(
            catalog.evaluate(&account, "cad.local.edit", 1)?,
            Decision::Allow
        );
        Ok(())
    }

    #[test]
    fn cancel_and_refund_return_the_account_to_student() -> TestResult {
        let catalog = catalog();
        let mut account = Account::new("p1", Tier::Professional);
        catalog.apply_event(
            &mut account,
            BillingEvent::Cancel {
                event_id: "evt-cancel".into(),
            },
        )?;
        assert_eq!(account.tier, Tier::Student);
        assert_eq!(account.payment, PaymentStatus::Cancelled);
        let mut account = Account::new("p2", Tier::Professional);
        catalog.apply_event(
            &mut account,
            BillingEvent::Refund {
                event_id: "evt-refund".into(),
            },
        )?;
        assert_eq!(account.payment, PaymentStatus::Refunded);
        assert_eq!(account.tier, Tier::Student);
        Ok(())
    }

    #[test]
    fn downgrade_reduces_included_storage() -> TestResult {
        let catalog = catalog();
        let mut account = Account::new("p3", Tier::Professional);
        account.consume("storage.cloud.gb", 10);
        assert_eq!(
            catalog.evaluate(&account, "storage.cloud.gb", 0)?,
            Decision::Allow
        );
        catalog.apply_event(
            &mut account,
            BillingEvent::Downgrade {
                event_id: "evt-down".into(),
                to: Tier::Creator,
            },
        )?;
        assert_eq!(
            catalog.evaluate(&account, "storage.cloud.gb", 0)?,
            Decision::Deny
        );
        Ok(())
    }

    #[test]
    fn student_has_no_ai_quota() -> TestResult {
        let catalog = catalog();
        let account = Account::new("s3", Tier::Student);
        assert!(catalog.evaluate(&account, "ai.assist", 1).is_err());
        Ok(())
    }

    #[test]
    fn target_prices_are_the_published_hypotheses() {
        assert_eq!(CREATOR_INR_PER_MONTH, 399);
        assert_eq!(PROFESSIONAL_INR_PER_MONTH, 999);
    }
}
