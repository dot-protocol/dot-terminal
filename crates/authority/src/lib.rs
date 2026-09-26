//! Experimental operation authorization. No secret access or transport is exposed.
//! The embedding broker supplies trusted policy/time, the node's signing key and a tip anchor.
use ring::{digest, signature};
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
    Journal(#[from] std::io::Error),
    /// The journal failed verification. Nothing is authorized until an owner looks.
    #[error("journal failed verification: {0}")]
    Tampered(&'static str),
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

/// Where a journal's tip (last sequence number and record hash) is kept apart from the journal file, so
/// a rollback of the file (restoring an old copy, deleting the tail) contradicts it. The tip only moves
/// forward: `advance` must refuse a lower sequence. Release 1 wiring picks the anchor (see #dot K4).
pub trait TipAnchor: Send {
    fn tip(&self) -> Result<Option<Tip>, Error>;
    fn advance(&mut self, tip: Tip) -> Result<(), Error>;
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tip {
    pub seq: u64,
    pub hash: [u8; 32],
}
/// An in-process anchor, shared by clones, for tests and single-process embedding. It survives a
/// journal being dropped and reopened, which is what the rollback tests need.
#[derive(Clone, Default)]
pub struct MemoryAnchor(std::sync::Arc<std::sync::Mutex<Option<Tip>>>);
impl TipAnchor for MemoryAnchor {
    fn tip(&self) -> Result<Option<Tip>, Error> {
        Ok(*self.0.lock().unwrap_or_else(|e| e.into_inner()))
    }
    fn advance(&mut self, tip: Tip) -> Result<(), Error> {
        let mut t = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if t.is_some_and(|old| tip.seq < old.seq) {
            return Err(Error::Tampered("the anchored tip never moves backwards"));
        }
        *t = Some(tip);
        Ok(())
    }
}

/// One line of the authority journal (#dot K4). Signed by the node's key, which is never stored in or
/// beside the journal: someone who can rewrite the file can recompute hashes, but not signatures.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Line {
    seq: u64,
    prev: [u8; 32],
    grant: [u8; 32],
    /// The grant's signed terms. The first reservation binds the id to them, so re-signed terms under
    /// the same id cannot reset its budget.
    grant_hash: [u8; 32],
    nonce: [u8; 32],
    body: [u8; 32],
    event: Event,
    max_uses: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Event {
    Reserve,
    Succeeded,
    Failed,
    Unknown,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    line: Line,
    #[serde(with = "sig_hex")]
    sig: Vec<u8>,
}
mod sig_hex {
    use serde::{Deserialize, Deserializer, Serializer};
    pub fn serialize<S: Serializer>(v: &[u8], s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&v.iter().map(|b| format!("{b:02x}")).collect::<String>())
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        let t = String::deserialize(d)?;
        (0..t.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(t.get(i..i + 2).unwrap_or("zz"), 16))
            .collect::<Result<_, _>>()
            .map_err(serde::de::Error::custom)
    }
}
fn line_message(line: &Line) -> Result<Vec<u8>, Error> {
    encode(b"DOT-AUTH-LEDGER-LINE-ED25519-V1\0", line)
}
fn record_hash(line: &Line, sig: &[u8]) -> Result<[u8; 32], Error> {
    let mut bytes = line_message(line)?;
    bytes.extend_from_slice(sig);
    Ok(digest::digest(&digest::SHA256, &bytes)
        .as_ref()
        .try_into()
        .unwrap())
}

type Digest = [u8; 32];
/// (grant id, nonce): one operation.
type OpKey = (Digest, Digest);
/// The one authority journal: an append-only file of signed, hash-chained lines. Opening it (and every
/// reservation) verifies each line's signature, contiguous sequence and chain, and that the file reaches
/// the anchored tip. Any break fails closed: no permit is released until an owner looks. Spent uses are
/// counted from signed reserve lines, never from a counter, so deleting lines cannot re-arm a grant.
/// Metadata is not encrypted; bodies are only digests.
pub struct Journal {
    file: std::fs::File,
    key: signature::Ed25519KeyPair,
    anchor: Box<dyn TipAnchor>,
    read_to: u64,
    tip: Option<Tip>,
    grants: std::collections::HashMap<[u8; 32], ([u8; 32], u32)>,
    ops: std::collections::HashMap<OpKey, (Digest, Event)>,
}
pub struct Permit {
    grant: [u8; 32],
    nonce: [u8; 32],
    request_hash: [u8; 32],
}
impl Journal {
    /// `key` is the node's signing key; it must not live in the journal's directory.
    pub fn open(
        path: &std::path::Path,
        key: signature::Ed25519KeyPair,
        anchor: Box<dyn TipAnchor>,
    ) -> Result<Self, Error> {
        let file = std::fs::OpenOptions::new()
            .read(true)
            .append(true)
            .create(true)
            .open(path)
            .or_else(|_| std::fs::File::open(path))?;
        let mut journal = Self {
            file,
            key,
            anchor,
            read_to: 0,
            tip: None,
            grants: Default::default(),
            ops: Default::default(),
        };
        journal.file.lock_shared()?;
        let caught_up = journal.catch_up();
        journal.file.unlock()?;
        caught_up?;
        Ok(journal)
    }
    /// Read and verify every line written since the last read (by this or another process), then check
    /// the file reaches the anchored tip. Caller holds the file lock.
    fn catch_up(&mut self) -> Result<(), Error> {
        use std::io::{Read, Seek, SeekFrom};
        let len = self.file.metadata()?.len();
        if len < self.read_to {
            return Err(Error::Tampered("the journal is shorter than already read"));
        }
        self.file.seek(SeekFrom::Start(self.read_to))?;
        let mut rest = Vec::new();
        (&self.file)
            .take(len - self.read_to)
            .read_to_end(&mut rest)?;
        let public = signature::KeyPair::public_key(&self.key).as_ref().to_vec();
        let mut at = 0usize;
        while at < rest.len() {
            let size = rest
                .get(at..at + 4)
                .map(|b| u32::from_be_bytes(b.try_into().unwrap()) as usize)
                .ok_or(Error::Tampered("a truncated record header"))?;
            let body = rest
                .get(at + 4..at + 4 + size)
                .ok_or(Error::Tampered("a truncated record"))?;
            let record: Record = serde_json::from_slice(body)
                .map_err(|_| Error::Tampered("an unreadable record"))?;
            let line = &record.line;
            let expected = self.tip.map_or(1, |t| t.seq + 1);
            let prev = self.tip.map_or([0; 32], |t| t.hash);
            if line.seq != expected || line.prev != prev {
                return Err(Error::Tampered("a gap or a broken chain"));
            }
            signature::UnparsedPublicKey::new(&signature::ED25519, &public)
                .verify(&line_message(line)?, &record.sig)
                .map_err(|_| Error::Tampered("a line not signed by this node"))?;
            self.apply(line)?;
            self.tip = Some(Tip {
                seq: line.seq,
                hash: record_hash(line, &record.sig)?,
            });
            at += 4 + size;
        }
        self.read_to = len;
        if let Some(anchored) = self.anchor.tip()? {
            match self.tip {
                Some(t) if t.seq >= anchored.seq => {
                    if t.seq == anchored.seq && t.hash != anchored.hash {
                        return Err(Error::Tampered(
                            "the journal diverges from its anchored tip",
                        ));
                    }
                }
                _ => {
                    return Err(Error::Tampered(
                        "the journal is behind its anchored tip (rolled back or cut)",
                    ));
                }
            }
        }
        Ok(())
    }
    fn apply(&mut self, line: &Line) -> Result<(), Error> {
        let key = (line.grant, line.nonce);
        match line.event {
            Event::Reserve => {
                let entry = self
                    .grants
                    .entry(line.grant)
                    .or_insert((line.grant_hash, 0));
                if entry.0 != line.grant_hash
                    || entry.1 >= line.max_uses
                    || self.ops.contains_key(&key)
                {
                    return Err(Error::Tampered(
                        "a reservation the journal could never have granted",
                    ));
                }
                entry.1 += 1;
                self.ops.insert(key, (line.body, Event::Reserve));
            }
            outcome => match self.ops.get_mut(&key) {
                Some((body, state)) if *body == line.body && *state == Event::Reserve => {
                    *state = outcome
                }
                _ => return Err(Error::Tampered("an outcome for no pending reservation")),
            },
        }
        Ok(())
    }
    /// Sign, append and fsync one line, then move the anchor. The anchor moves only after the line is
    /// durable, so the anchor never points past the file.
    fn append(&mut self, mut line: Line) -> Result<(), Error> {
        use std::io::Write;
        line.seq = self.tip.map_or(1, |t| t.seq + 1);
        line.prev = self.tip.map_or([0; 32], |t| t.hash);
        let sig = self.key.sign(&line_message(&line)?).as_ref().to_vec();
        let hash = record_hash(&line, &sig)?;
        let bytes = serde_json::to_vec(&Record {
            line: line.clone(),
            sig,
        })?;
        let mut framed = (bytes.len() as u32).to_be_bytes().to_vec();
        framed.extend(bytes);
        self.file.write_all(&framed)?;
        self.file.sync_all()?;
        self.apply(&line)?;
        self.read_to += framed.len() as u64;
        let tip = Tip {
            seq: line.seq,
            hash,
        };
        self.tip = Some(tip);
        self.anchor.advance(tip)
    }
    /// Policy must come from the broker, held stable through this call. A pending intent consumes
    /// budget even after a crash. No external operation is retried here.
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
        self.file.lock()?;
        let reserved = self.reserve(verified);
        self.file.unlock()?;
        reserved
    }
    fn reserve(&mut self, v: Verified) -> Result<Permit, Error> {
        self.catch_up()?;
        let used = self.grants.get(&v.grant);
        if used.is_some_and(|(hash, n)| *hash != v.grant_hash || *n >= v.max_uses)
            || self.ops.contains_key(&(v.grant, v.nonce))
        {
            return Err(Error::Spent);
        }
        self.append(Line {
            seq: 0,
            prev: [0; 32],
            grant: v.grant,
            grant_hash: v.grant_hash,
            nonce: v.nonce,
            body: v.request_hash,
            event: Event::Reserve,
            max_uses: v.max_uses,
        })?;
        Ok(Permit {
            grant: v.grant,
            nonce: v.nonce,
            request_hash: v.request_hash,
        })
    }
    pub fn finish(&mut self, permit: Permit, outcome: Outcome) -> Result<(), Error> {
        self.file.lock()?;
        let done = self.catch_up().and_then(|()| {
            match self.ops.get(&(permit.grant, permit.nonce)) {
                Some((body, Event::Reserve)) if *body == permit.request_hash => {}
                _ => return Err(Error::NotPending),
            }
            let (grant_hash, _) = self.grants[&permit.grant];
            self.append(Line {
                seq: 0,
                prev: [0; 32],
                grant: permit.grant,
                grant_hash,
                nonce: permit.nonce,
                body: permit.request_hash,
                event: match outcome {
                    Outcome::Succeeded => Event::Succeeded,
                    Outcome::Failed => Event::Failed,
                    Outcome::Unknown => Event::Unknown,
                },
                max_uses: 0,
            })
        });
        self.file.unlock()?;
        done
    }
    pub fn pending_count(&self) -> Result<u64, Error> {
        Ok(self
            .ops
            .values()
            .filter(|(_, e)| *e == Event::Reserve)
            .count() as u64)
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
    fn node() -> Ed25519KeyPair {
        Ed25519KeyPair::from_seed_unchecked(&[9; 32]).unwrap()
    }
    fn open(path: &std::path::Path, anchor: &MemoryAnchor) -> Result<Journal, Error> {
        Journal::open(path, node(), Box::new(anchor.clone()))
    }
    /// The journal's records as (header+json) byte ranges, for tests that rewrite the file.
    fn records(path: &std::path::Path) -> Vec<Vec<u8>> {
        let bytes = std::fs::read(path).unwrap();
        let (mut at, mut out) = (0, vec![]);
        while at < bytes.len() {
            let n = u32::from_be_bytes(bytes[at..at + 4].try_into().unwrap()) as usize;
            out.push(bytes[at + 4..at + 4 + n].to_vec());
            at += 4 + n;
        }
        out
    }
    fn write_records(path: &std::path::Path, records: &[Vec<u8>]) {
        let mut bytes = vec![];
        for r in records {
            bytes.extend((r.len() as u32).to_be_bytes());
            bytes.extend(r);
        }
        std::fs::write(path, bytes).unwrap();
    }
    #[test]
    fn restart_replay_budget_and_ambiguous_outcomes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("authority.journal");
        let anchor = MemoryAnchor::default();
        let mut f = Fixture::new();
        let mut journal = open(&path, &anchor).unwrap();
        let _unfinished = f.authorize(&mut journal).unwrap();
        assert_eq!(journal.pending_count().unwrap(), 1);
        // Simulated interruption after reservation: never retry automatically.
        drop(journal);
        let mut journal = open(&path, &anchor).unwrap();
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
        let path = dir.path().join("authority.journal");
        let anchor = MemoryAnchor::default();
        let mut f = Fixture::new();
        f.grant.max_uses = 1;
        let mut journal = open(&path, &anchor).unwrap();
        f.request.action = "export".into();
        assert!(matches!(f.authorize(&mut journal), Err(Error::Denied)));
        assert_eq!(journal.pending_count().unwrap(), 0);
        f.request.action = "invoke".into();
        // A handle that cannot write, even for root (a 0444 file is still writable by root).
        let mut readonly = open(&path, &anchor).unwrap();
        readonly.file = std::fs::File::open(&path).unwrap();
        assert!(matches!(f.authorize(&mut readonly), Err(Error::Journal(_))));
        assert_eq!(readonly.pending_count().unwrap(), 0);
        assert!(f.authorize(&mut journal).is_ok());
    }
    #[test]
    fn concurrent_connections_cannot_overspend() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("authority.journal");
        let anchor = MemoryAnchor::default();
        open(&path, &anchor).unwrap();
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
        let threads: Vec<_> = (0..2)
            .map(|n| {
                let (path, barrier, anchor) = (path.clone(), barrier.clone(), anchor.clone());
                std::thread::spawn(move || {
                    let mut f = Fixture::new();
                    f.grant.max_uses = 1;
                    f.request.nonce = [n; 32];
                    let mut journal = open(&path, &anchor).unwrap();
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
    /// Jobs' T1 probe: a raw edit turned "failed" into "succeeded" and the journal served it. Here the
    /// edit is made properly (valid JSON, correct framing) and still fails: the signature covers it.
    #[test]
    fn an_edited_outcome_fails_verification() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("authority.journal");
        let anchor = MemoryAnchor::default();
        let f = Fixture::new();
        let mut journal = open(&path, &anchor).unwrap();
        let permit = f.authorize(&mut journal).unwrap();
        journal.finish(permit, Outcome::Failed).unwrap();
        drop(journal);
        let mut recs = records(&path);
        let mut last: serde_json::Value = serde_json::from_slice(&recs[1]).unwrap();
        assert_eq!(last["line"]["event"], "failed");
        last["line"]["event"] = "succeeded".into();
        recs[1] = serde_json::to_vec(&last).unwrap();
        write_records(&path, &recs);
        assert!(matches!(open(&path, &anchor), Err(Error::Tampered(_))));
        // A fresh anchor does not help the forger: the signature alone refuses it.
        assert!(matches!(
            open(&path, &MemoryAnchor::default()),
            Err(Error::Tampered(_))
        ));
    }
    /// Jobs' T1 probe: deleting rows re-armed a spent max_uses=1 grant and its nonce. Cutting the tail,
    /// or restoring an older copy of the whole file, now contradicts the anchored tip.
    #[test]
    fn deleting_lines_or_restoring_an_old_copy_cannot_re_arm_a_grant() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("authority.journal");
        let anchor = MemoryAnchor::default();
        let mut f = Fixture::new();
        f.grant.max_uses = 1;
        let mut journal = open(&path, &anchor).unwrap();
        let before = std::fs::read(&path).unwrap(); // an old copy, taken before the use
        let permit = f.authorize(&mut journal).unwrap();
        journal.finish(permit, Outcome::Succeeded).unwrap();
        drop(journal);
        let recs = records(&path);
        write_records(&path, &recs[..1]); // cut the outcome
        assert!(matches!(open(&path, &anchor), Err(Error::Tampered(_))));
        std::fs::write(&path, &before).unwrap(); // restore the pre-use copy
        assert!(matches!(open(&path, &anchor), Err(Error::Tampered(_))));
        write_records(&path, &recs); // the true journal still opens, and the grant stays spent
        let mut journal = open(&path, &anchor).unwrap();
        f.request.nonce = [6; 32];
        assert!(matches!(f.authorize(&mut journal), Err(Error::Spent)));
    }
    /// A line signed by any key but this node's is refused, even when correctly chained and framed.
    #[test]
    fn a_line_signed_by_another_key_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("authority.journal");
        let f = Fixture::new();
        let other = Ed25519KeyPair::from_seed_unchecked(&[8; 32]).unwrap();
        let mut forged = Journal::open(&path, other, Box::new(MemoryAnchor::default())).unwrap();
        f.authorize(&mut forged).unwrap();
        drop(forged);
        assert!(matches!(
            open(&path, &MemoryAnchor::default()),
            Err(Error::Tampered(_))
        ));
    }
    #[test]
    fn the_anchor_never_moves_backwards() {
        let mut a = MemoryAnchor::default();
        a.advance(Tip {
            seq: 5,
            hash: [1; 32],
        })
        .unwrap();
        assert!(
            a.advance(Tip {
                seq: 4,
                hash: [2; 32]
            })
            .is_err()
        );
        assert_eq!(a.tip().unwrap().unwrap().seq, 5);
    }
}
