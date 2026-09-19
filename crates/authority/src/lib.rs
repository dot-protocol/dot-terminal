//! Experimental operation authorization. No secret access or transport is exposed.
//! The embedding broker supplies trusted policy/time and a private SQLite connection.
use ring::{digest, signature};
use rusqlite::{Connection, TransactionBehavior, params};
use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("authorization denied")]
    Denied,
    #[error("operation already reserved or grant exhausted")]
    Spent,
    #[error("operation not pending")]
    NotPending,
    #[error("journal failure: {0}")]
    Journal(#[from] rusqlite::Error),
    #[error("invalid encoding: {0}")]
    Encoding(#[from] serde_json::Error),
}

/// Version 1 supports direct owner grants only. No implicit delegation or wildcards.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Grant {
    pub version: u8,
    pub id: [u8; 32],
    pub subject: [u8; 32],
    pub run: [u8; 32],
    pub audience: String,
    pub resource: String,
    pub action: String,
    pub not_before: u64,
    pub expires: u64,
    pub policy_epoch: u64,
    pub max_uses: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub version: u8,
    pub grant_id: [u8; 32],
    pub run: [u8; 32],
    /// Unique per operation. The broker journals this before permitting execution.
    pub nonce: [u8; 32],
    pub audience: String,
    pub resource: String,
    pub action: String,
    pub body_sha256: [u8; 32],
}

/// Supplied by trusted broker configuration, never by the requesting agent.
pub struct Policy<'a> {
    pub owner_key: &'a [u8; 32],
    pub audience: &'a str,
    pub epoch: u64,
    pub now: u64,
    pub revoked: &'a [[u8; 32]],
}

// These typed structs have a fixed field order and integer-only numeric fields.
// Sign re-encoded typed values, not arbitrary JSON bytes. Version changes require
// a new domain; unknown fields are rejected on decoding.
fn encode<T: Serialize>(domain: &[u8], value: &T) -> Result<Vec<u8>, Error> {
    let mut bytes = domain.to_vec();
    bytes.extend(serde_json::to_vec(value)?);
    Ok(bytes)
}
pub fn grant_message(grant: &Grant) -> Result<Vec<u8>, Error> {
    encode(b"DOT-AUTH-GRANT-ED25519-V1\0", grant)
}
pub fn request_message(request: &Request) -> Result<Vec<u8>, Error> {
    encode(b"DOT-AUTH-REQUEST-ED25519-V1\0", request)
}
fn valid_text(s: &str) -> bool {
    !s.is_empty() && s.len() <= 256 && !s.chars().any(char::is_control)
}

/// A verified operation is opaque and binds the exact signed body digest.
/// Only `Journal::authorize` releases a permit. Do not dispatch after verify alone.
pub struct Verified {
    grant: [u8; 32],
    grant_hash: [u8; 32],
    nonce: [u8; 32],
    request_hash: [u8; 32],
    max_uses: u32,
}

pub fn verify(
    grant: &Grant,
    owner_signature: &[u8],
    request: &Request,
    subject_signature: &[u8],
    body: &[u8],
    policy: &Policy<'_>,
) -> Result<Verified, Error> {
    if grant.version != 1
        || request.version != 1
        || grant.max_uses == 0
        || grant.not_before >= grant.expires
        || policy.now < grant.not_before
        || policy.now >= grant.expires
        || grant.policy_epoch != policy.epoch
        || policy.revoked.contains(&grant.id)
        || grant.audience != policy.audience
        || request.audience != grant.audience
        || request.resource != grant.resource
        || request.action != grant.action
        || request.run != grant.run
        || request.grant_id != grant.id
        || ![&grant.audience, &grant.resource, &grant.action]
            .iter()
            .all(|s| valid_text(s))
        || digest::digest(&digest::SHA256, body).as_ref() != request.body_sha256
    {
        return Err(Error::Denied);
    }
    let grant_bytes = grant_message(grant)?;
    let request_bytes = request_message(request)?;
    signature::UnparsedPublicKey::new(&signature::ED25519, policy.owner_key)
        .verify(&grant_bytes, owner_signature)
        .map_err(|_| Error::Denied)?;
    signature::UnparsedPublicKey::new(&signature::ED25519, grant.subject)
        .verify(&request_bytes, subject_signature)
        .map_err(|_| Error::Denied)?;
    Ok(Verified {
        grant: grant.id,
        grant_hash: digest::digest(&digest::SHA256, &grant_bytes)
            .as_ref()
            .try_into()
            .unwrap(),
        nonce: request.nonce,
        request_hash: digest::digest(&digest::SHA256, &request_bytes)
            .as_ref()
            .try_into()
            .unwrap(),
        max_uses: grant.max_uses,
    })
}

/// Journal metadata is not encrypted. Experimental: use synthetic operations only.
/// The caller must secure the database and parent directory; no production vault hookup.
pub struct Journal(Connection);
pub struct Permit {
    grant: [u8; 32],
    nonce: [u8; 32],
    request_hash: [u8; 32],
}
impl Journal {
    pub fn new(connection: Connection) -> Result<Self, Error> {
        connection.busy_timeout(std::time::Duration::from_secs(5))?;
        connection.execute_batch(
            "PRAGMA synchronous=FULL;
            CREATE TABLE IF NOT EXISTS authority_grants (
                id BLOB PRIMARY KEY, digest BLOB NOT NULL, uses INTEGER NOT NULL);
            CREATE TABLE IF NOT EXISTS authority_operations (
                grant_id BLOB NOT NULL, nonce BLOB NOT NULL, digest BLOB NOT NULL,
                outcome TEXT NOT NULL CHECK(outcome IN ('pending','succeeded','failed','unknown')),
                PRIMARY KEY(grant_id, nonce));",
        )?;
        Ok(Self(connection))
    }
    /// Policy must come from the broker, held stable through this call. A pending
    /// intent consumes budget even after crash. No external operation is retried here.
    pub fn authorize(
        &mut self,
        grant: &Grant,
        owner_signature: &[u8],
        request: &Request,
        subject_signature: &[u8],
        body: &[u8],
        policy: &Policy<'_>,
    ) -> Result<Permit, Error> {
        let verified = verify(
            grant,
            owner_signature,
            request,
            subject_signature,
            body,
            policy,
        )?;
        self.reserve(verified)
    }
    fn reserve(&mut self, verified: Verified) -> Result<Permit, Error> {
        let tx = self
            .0
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute(
            "INSERT OR IGNORE INTO authority_grants VALUES (?1,?2,0)",
            params![verified.grant, verified.grant_hash],
        )?;
        let changed = tx.execute(
            "UPDATE authority_grants SET uses=uses+1
            WHERE id=?1 AND digest=?2 AND uses < ?3",
            params![verified.grant, verified.grant_hash, verified.max_uses],
        )?;
        if changed != 1 {
            return Err(Error::Spent);
        }
        let inserted = tx.execute(
            "INSERT OR IGNORE INTO authority_operations VALUES (?1,?2,?3,'pending')",
            params![verified.grant, verified.nonce, verified.request_hash],
        )?;
        if inserted != 1 {
            return Err(Error::Spent);
        }
        tx.commit()?;
        Ok(Permit {
            grant: verified.grant,
            nonce: verified.nonce,
            request_hash: verified.request_hash,
        })
    }
    pub fn finish(&mut self, permit: Permit, outcome: Outcome) -> Result<(), Error> {
        let text = match outcome {
            Outcome::Succeeded => "succeeded",
            Outcome::Failed => "failed",
            Outcome::Unknown => "unknown",
        };
        let changed = self.0.execute(
            "UPDATE authority_operations SET outcome=?4
            WHERE grant_id=?1 AND nonce=?2 AND digest=?3 AND outcome='pending'",
            params![permit.grant, permit.nonce, permit.request_hash, text],
        )?;
        if changed != 1 {
            return Err(Error::NotPending);
        }
        Ok(())
    }
    pub fn pending_count(&self) -> Result<u64, Error> {
        Ok(self.0.query_row(
            "SELECT count(*) FROM authority_operations WHERE outcome='pending'",
            [],
            |r| r.get(0),
        )?)
    }
}
pub enum Outcome {
    Succeeded,
    Failed,
    Unknown,
}

#[cfg(test)]
mod tests {
    use super::*;
    use ring::signature::{Ed25519KeyPair, KeyPair};
    struct Fixture {
        owner: Ed25519KeyPair,
        agent: Ed25519KeyPair,
        grant: Grant,
        request: Request,
    }
    impl Fixture {
        fn new() -> Self {
            let owner = Ed25519KeyPair::from_seed_unchecked(&[1; 32]).unwrap();
            let agent = Ed25519KeyPair::from_seed_unchecked(&[2; 32]).unwrap();
            let grant = Grant {
                version: 1,
                id: [3; 32],
                subject: agent.public_key().as_ref().try_into().unwrap(),
                run: [4; 32],
                audience: "test-broker".into(),
                resource: "synthetic-model".into(),
                action: "invoke".into(),
                not_before: 10,
                expires: 20,
                policy_epoch: 7,
                max_uses: 2,
            };
            let request = Request {
                version: 1,
                grant_id: grant.id,
                run: grant.run,
                nonce: [5; 32],
                audience: grant.audience.clone(),
                resource: grant.resource.clone(),
                action: grant.action.clone(),
                body_sha256: digest::digest(&digest::SHA256, b"synthetic")
                    .as_ref()
                    .try_into()
                    .unwrap(),
            };
            Self {
                owner,
                agent,
                grant,
                request,
            }
        }
        fn policy(&self) -> Policy<'_> {
            Policy {
                owner_key: self.owner.public_key().as_ref().try_into().unwrap(),
                audience: "test-broker",
                epoch: 7,
                now: 15,
                revoked: &[],
            }
        }
        fn check(&self, policy: &Policy<'_>) -> Result<Verified, Error> {
            verify(
                &self.grant,
                self.owner.sign(&grant_message(&self.grant)?).as_ref(),
                &self.request,
                self.agent.sign(&request_message(&self.request)?).as_ref(),
                b"synthetic",
                policy,
            )
        }
        fn authorize(&self, journal: &mut Journal) -> Result<Permit, Error> {
            journal.authorize(
                &self.grant,
                self.owner.sign(&grant_message(&self.grant)?).as_ref(),
                &self.request,
                self.agent.sign(&request_message(&self.request)?).as_ref(),
                b"synthetic",
                &self.policy(),
            )
        }
    }
    #[test]
    fn authority_scope_time_and_revocation() {
        let f = Fixture::new();
        assert!(f.check(&f.policy()).is_ok());
        for now in [0, 9, 20, 21, u64::MAX] {
            assert!(matches!(
                f.check(&Policy { now, ..f.policy() }),
                Err(Error::Denied)
            ));
        }
        assert!(
            f.check(&Policy {
                now: 10,
                ..f.policy()
            })
            .is_ok()
        );
        assert!(
            f.check(&Policy {
                epoch: 8,
                ..f.policy()
            })
            .is_err()
        );
        assert!(
            f.check(&Policy {
                audience: "other",
                ..f.policy()
            })
            .is_err()
        );
        assert!(
            f.check(&Policy {
                revoked: &[f.grant.id],
                ..f.policy()
            })
            .is_err()
        );
        assert!(
            f.check(&Policy {
                owner_key: &[9; 32],
                ..f.policy()
            })
            .is_err()
        );
        for field in 0..6 {
            let mut f = Fixture::new();
            match field {
                0 => f.request.action = "export".into(),
                1 => f.request.resource = "other".into(),
                2 => f.request.run = [8; 32],
                3 => f.request.grant_id = [8; 32],
                4 => f.request.body_sha256 = [8; 32],
                _ => f.request.version = 2,
            }
            assert!(f.check(&f.policy()).is_err());
        }
    }
    #[test]
    fn signatures_bind_grant_and_request() {
        let mut f = Fixture::new();
        let gs = f.owner.sign(&grant_message(&f.grant).unwrap());
        let rs = f.agent.sign(&request_message(&f.request).unwrap());
        f.grant.max_uses += 1;
        assert!(
            verify(
                &f.grant,
                gs.as_ref(),
                &f.request,
                rs.as_ref(),
                b"synthetic",
                &f.policy()
            )
            .is_err()
        );
        f.grant.max_uses -= 1;
        f.request.nonce = [9; 32];
        assert!(
            verify(
                &f.grant,
                gs.as_ref(),
                &f.request,
                rs.as_ref(),
                b"synthetic",
                &f.policy()
            )
            .is_err()
        );
        let impostor = f.owner.sign(&request_message(&f.request).unwrap());
        assert!(
            verify(
                &f.grant,
                gs.as_ref(),
                &f.request,
                impostor.as_ref(),
                b"synthetic",
                &f.policy()
            )
            .is_err()
        );
        let mut value = serde_json::to_value(&f.grant).unwrap();
        value["delegation"] = serde_json::json!(true);
        assert!(serde_json::from_value::<Grant>(value).is_err());
    }
    #[test]
    fn restart_replay_budget_and_ambiguous_outcomes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("synthetic.sqlite");
        let mut f = Fixture::new();
        let mut journal = Journal::new(Connection::open(&path).unwrap()).unwrap();
        let _unfinished = f.authorize(&mut journal).unwrap();
        assert_eq!(journal.pending_count().unwrap(), 1);
        // Simulated interruption after reservation: never retry automatically.
        drop(journal);
        let mut journal = Journal::new(Connection::open(&path).unwrap()).unwrap();
        assert_eq!(journal.pending_count().unwrap(), 1);
        assert!(matches!(f.authorize(&mut journal), Err(Error::Spent)));
        f.request.nonce = [6; 32];
        let permit = f.authorize(&mut journal).unwrap(); // failed replay did not burn a use
        journal.finish(permit, Outcome::Succeeded).unwrap();
        f.request.nonce = [7; 32];
        assert!(matches!(f.authorize(&mut journal), Err(Error::Spent)));
        f.grant.max_uses = 100; // same ID with changed signed terms cannot reset budget
        assert!(matches!(f.authorize(&mut journal), Err(Error::Spent)));
    }
    #[test]
    fn denial_and_storage_failure_never_release_a_permit() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("synthetic.sqlite");
        let mut f = Fixture::new();
        f.grant.max_uses = 1;
        let mut journal = Journal::new(Connection::open(&path).unwrap()).unwrap();
        f.request.action = "export".into();
        assert!(matches!(f.authorize(&mut journal), Err(Error::Denied)));
        assert_eq!(journal.pending_count().unwrap(), 0);
        f.request.action = "invoke".into();
        let readonly =
            Connection::open_with_flags(&path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
        let mut readonly = Journal::new(readonly).unwrap();
        assert!(matches!(f.authorize(&mut readonly), Err(Error::Journal(_))));
        assert_eq!(journal.pending_count().unwrap(), 0);
        assert!(f.authorize(&mut journal).is_ok());
    }

    #[test]
    fn concurrent_connections_cannot_overspend() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("synthetic.sqlite");
        Journal::new(Connection::open(&path).unwrap()).unwrap();
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
        let threads: Vec<_> = (0..2)
            .map(|n| {
                let path = path.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    let mut f = Fixture::new();
                    f.grant.max_uses = 1;
                    f.request.nonce = [n; 32];
                    let mut journal = Journal::new(Connection::open(path).unwrap()).unwrap();
                    barrier.wait();
                    match f.authorize(&mut journal) {
                        Ok(_) => 1,
                        Err(Error::Spent) => 0,
                        Err(e) => panic!("{e}"),
                    }
                })
            })
            .collect();
        assert_eq!(
            threads.into_iter().map(|t| t.join().unwrap()).sum::<u8>(),
            1
        );
    }
}
