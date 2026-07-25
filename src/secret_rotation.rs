use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Identifies the secret family and backing service a credential belongs to.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
pub enum SecretKind {
    DatabaseCredential,
    ApiKey,
}

/// Rotation policy for one class of secrets.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RotationPolicy {
    pub rotate_after: Duration,
    pub overlap: Duration,
    pub canary_percent: u8,
}

impl RotationPolicy {
    pub fn validate(&self) -> Result<(), RotationError> {
        if self.rotate_after <= Duration::zero() {
            return Err(RotationError::InvalidPolicy(
                "rotate_after must be positive",
            ));
        }
        if self.overlap < Duration::zero() || self.overlap >= self.rotate_after {
            return Err(RotationError::InvalidPolicy(
                "overlap must be non-negative and shorter than rotate_after",
            ));
        }
        if !(1..=100).contains(&self.canary_percent) {
            return Err(RotationError::InvalidPolicy(
                "canary_percent must be 1..=100",
            ));
        }
        Ok(())
    }
}

/// Metadata intentionally excludes secret material so it can be logged and exported safely.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SecretRecord {
    pub id: String,
    pub kind: SecretKind,
    pub version: u64,
    pub active_from: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub previous_version_expires_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RotationDecision {
    NotDue,
    Rotate { rollout: RolloutPlan },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RolloutPlan {
    pub secret_id: String,
    pub next_version: u64,
    pub canary_percent: u8,
    pub full_cutover_at: DateTime<Utc>,
    pub retire_previous_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RotationError {
    InvalidPolicy(&'static str),
    MissingSecret(String),
    ProviderFailed(String),
}

pub trait SecretProvider {
    fn create_next_version(&mut self, record: &SecretRecord) -> Result<u64, RotationError>;
    fn retire_version(&mut self, secret_id: &str, version: u64) -> Result<(), RotationError>;
}

/// In-memory coordinator for deterministic rotation decisions. Production callers wrap this with
/// durable storage and a KMS/secret-manager implementation of [`SecretProvider`].
#[derive(Default)]
pub struct SecretRotationService {
    records: BTreeMap<String, SecretRecord>,
}

impl SecretRotationService {
    pub fn insert(&mut self, record: SecretRecord) {
        self.records.insert(record.id.clone(), record);
    }

    pub fn record(&self, id: &str) -> Option<&SecretRecord> {
        self.records.get(id)
    }

    pub fn evaluate(
        &self,
        id: &str,
        policy: &RotationPolicy,
        now: DateTime<Utc>,
    ) -> Result<RotationDecision, RotationError> {
        policy.validate()?;
        let record = self
            .records
            .get(id)
            .ok_or_else(|| RotationError::MissingSecret(id.into()))?;
        let rotate_at = record.expires_at - policy.overlap;
        if now < rotate_at {
            return Ok(RotationDecision::NotDue);
        }
        Ok(RotationDecision::Rotate {
            rollout: RolloutPlan {
                secret_id: record.id.clone(),
                next_version: record.version + 1,
                canary_percent: policy.canary_percent,
                full_cutover_at: now,
                retire_previous_at: now + policy.overlap,
            },
        })
    }

    pub fn rotate_due<P: SecretProvider>(
        &mut self,
        id: &str,
        policy: &RotationPolicy,
        now: DateTime<Utc>,
        provider: &mut P,
    ) -> Result<RotationDecision, RotationError> {
        let decision = self.evaluate(id, policy, now)?;
        let RotationDecision::Rotate { rollout } = decision.clone() else {
            return Ok(decision);
        };
        let current = self
            .records
            .get(id)
            .cloned()
            .ok_or_else(|| RotationError::MissingSecret(id.into()))?;
        let next_version = provider.create_next_version(&current)?;
        if next_version != rollout.next_version {
            return Err(RotationError::ProviderFailed(
                "provider returned unexpected version".into(),
            ));
        }
        provider.retire_version(&current.id, current.version)?;
        self.records.insert(
            id.into(),
            SecretRecord {
                version: next_version,
                active_from: now,
                expires_at: now + policy.rotate_after,
                previous_version_expires_at: Some(rollout.retire_previous_at),
                ..current
            },
        );
        Ok(decision)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FakeProvider {
        retired: Vec<(String, u64)>,
    }

    impl SecretProvider for FakeProvider {
        fn create_next_version(&mut self, record: &SecretRecord) -> Result<u64, RotationError> {
            Ok(record.version + 1)
        }
        fn retire_version(&mut self, secret_id: &str, version: u64) -> Result<(), RotationError> {
            self.retired.push((secret_id.to_owned(), version));
            Ok(())
        }
    }

    fn fixture(now: DateTime<Utc>) -> (SecretRotationService, RotationPolicy) {
        let mut service = SecretRotationService::default();
        service.insert(SecretRecord {
            id: "db/main".into(),
            kind: SecretKind::DatabaseCredential,
            version: 7,
            active_from: now - Duration::days(29),
            expires_at: now + Duration::hours(1),
            previous_version_expires_at: None,
        });
        (
            service,
            RotationPolicy {
                rotate_after: Duration::days(30),
                overlap: Duration::hours(2),
                canary_percent: 5,
            },
        )
    }

    #[test]
    fn rotates_inside_overlap_window_and_tracks_previous_retirement() {
        let now = Utc::now();
        let (mut service, policy) = fixture(now);
        let mut provider = FakeProvider {
            retired: Vec::new(),
        };
        let decision = service
            .rotate_due("db/main", &policy, now, &mut provider)
            .expect("rotation succeeds");
        assert!(matches!(decision, RotationDecision::Rotate { .. }));
        let record = service.record("db/main").expect("record exists");
        assert_eq!(record.version, 8);
        assert_eq!(record.expires_at, now + Duration::days(30));
        assert_eq!(
            record.previous_version_expires_at,
            Some(now + Duration::hours(2))
        );
        assert_eq!(provider.retired, vec![("db/main".into(), 7)]);
    }

    #[test]
    fn skips_rotation_before_overlap_window() {
        let now = Utc::now();
        let mut service = SecretRotationService::default();
        service.insert(SecretRecord {
            id: "api/billing".into(),
            kind: SecretKind::ApiKey,
            version: 2,
            active_from: now,
            expires_at: now + Duration::days(10),
            previous_version_expires_at: None,
        });
        let policy = RotationPolicy {
            rotate_after: Duration::days(30),
            overlap: Duration::hours(2),
            canary_percent: 10,
        };
        assert_eq!(
            service
                .evaluate("api/billing", &policy, now)
                .expect("valid"),
            RotationDecision::NotDue
        );
    }

    #[test]
    fn rejects_invalid_policy() {
        let now = Utc::now();
        let (service, _) = fixture(now);
        let policy = RotationPolicy {
            rotate_after: Duration::hours(1),
            overlap: Duration::hours(1),
            canary_percent: 0,
        };
        assert!(matches!(
            service.evaluate("db/main", &policy, now),
            Err(RotationError::InvalidPolicy(_))
        ));
    }
}
