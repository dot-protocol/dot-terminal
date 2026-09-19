//! Pure state machines: no filesystem, networking, wall clock, or model backend.
use std::collections::VecDeque;

pub struct History {
    bytes: VecDeque<u8>,
    capacity: usize,
    next: u64,
}
pub struct Chunk {
    pub start: u64,
    pub next: u64,
    pub gap: bool,
    pub data: Vec<u8>,
}
impl History {
    pub fn new(capacity: usize) -> Self {
        assert!(capacity > 0);
        Self {
            bytes: VecDeque::new(),
            capacity,
            next: 0,
        }
    }
    pub fn append(&mut self, data: &[u8]) {
        for b in data {
            if self.bytes.len() == self.capacity {
                self.bytes.pop_front();
            }
            self.bytes.push_back(*b);
        }
        self.next = self
            .next
            .checked_add(data.len() as u64)
            .expect("output sequence exhausted");
    }
    pub fn read(&self, after: u64, limit: usize) -> Result<Chunk, &'static str> {
        if after > self.next {
            return Err("cursor is ahead of session output");
        }
        let oldest = self.next - self.bytes.len() as u64;
        let start = after.max(oldest);
        let data: Vec<_> = self
            .bytes
            .iter()
            .skip((start - oldest) as usize)
            .take(limit)
            .copied()
            .collect();
        Ok(Chunk {
            start,
            next: start + data.len() as u64,
            gap: after < oldest,
            data,
        })
    }
}

/// Fencing generations are keeper-local; the random session ID scopes them.
/// Input sequence is strictly monotonic and never silently retried after uncertainty.
#[derive(Default)]
pub struct Controller {
    generation: u64,
    active: bool,
    last_sequence: u64,
    last_data: Vec<u8>,
    uncertain: bool,
    handoff: Option<(String, u64)>,
}
#[derive(Debug, PartialEq)]
pub enum InputDecision {
    Write,
    Duplicate,
}
impl Controller {
    pub fn acquire(&mut self, takeover: bool) -> Result<u64, &'static str> {
        if self.active && !takeover {
            return Err("session already controlled; explicit takeover required");
        }
        self.generation = self
            .generation
            .checked_add(1)
            .ok_or("generation exhausted")?;
        self.handoff = None;
        self.active = true;
        self.last_sequence = 0;
        self.last_data.clear();
        self.uncertain = false;
        Ok(self.generation)
    }
    pub fn check(&self, g: u64) -> Result<(), &'static str> {
        if !self.active || g != self.generation {
            Err("stale controller generation")
        } else {
            Ok(())
        }
    }
    pub fn release(&mut self, g: u64) -> Result<(), &'static str> {
        self.check(g)?;
        self.active = false;
        self.handoff = None;
        Ok(())
    }
    /// A ticket authorizes a one-time cooperative handoff within an authenticated session.
    pub fn offer_handoff(
        &mut self,
        generation: u64,
        ticket: String,
        now: u64,
    ) -> Result<(), &'static str> {
        self.check(generation)?;
        self.handoff = Some((ticket, now.saturating_add(120)));
        Ok(())
    }
    pub fn cancel_handoff(&mut self, generation: u64) -> Result<(), &'static str> {
        self.check(generation)?;
        self.handoff = None;
        Ok(())
    }
    pub fn accept_handoff(&mut self, ticket: &str, now: u64) -> Result<u64, &'static str> {
        let Some((expected, deadline)) = &self.handoff else {
            return Err("handoff unavailable");
        };
        if now >= *deadline {
            self.handoff = None;
            return Err("handoff expired");
        }
        if ticket != expected {
            return Err("handoff unavailable");
        }
        self.acquire(true)
    }
    pub fn prepare(
        &mut self,
        g: u64,
        seq: u64,
        data: &[u8],
    ) -> Result<InputDecision, &'static str> {
        self.check(g)?;
        if self.uncertain {
            return Err("input outcome unknown; inspect session and explicitly take control again");
        }
        if seq == self.last_sequence && seq != 0 && data == self.last_data {
            return Ok(InputDecision::Duplicate);
        }
        if seq
            != self
                .last_sequence
                .checked_add(1)
                .ok_or("input sequence exhausted")?
        {
            return Err("out-of-order or conflicting input");
        }
        self.last_sequence = seq;
        self.last_data = data.to_vec();
        self.uncertain = true;
        Ok(InputDecision::Write)
    }
    pub fn written(&mut self) {
        self.uncertain = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn history_reports_truncation_and_exact_cursor() {
        let mut h = History::new(4);
        h.append(b"abcdef");
        let c = h.read(0, 2).unwrap();
        assert!(c.gap);
        assert_eq!((c.start, c.next, c.data), (2, 4, b"cd".to_vec()));
        assert_eq!(h.read(4, 10).unwrap().data, b"ef");
        assert!(h.read(7, 10).is_err());
    }
    #[test]
    fn takeover_fences_old_writer() {
        let mut c = Controller::default();
        let a = c.acquire(false).unwrap();
        assert!(c.acquire(false).is_err());
        let b = c.acquire(true).unwrap();
        assert!(c.check(a).is_err());
        assert!(c.check(b).is_ok());
    }
    #[test]
    fn dedup_never_reexecutes_acknowledged_input() {
        let mut c = Controller::default();
        let g = c.acquire(false).unwrap();
        assert_eq!(c.prepare(g, 1, b"a"), Ok(InputDecision::Write));
        c.written();
        assert_eq!(c.prepare(g, 1, b"a"), Ok(InputDecision::Duplicate));
        assert!(c.prepare(g, 1, b"b").is_err());
        assert!(c.prepare(g, 3, b"a").is_err());
    }
    #[test]
    fn partial_write_is_not_retried() {
        let mut c = Controller::default();
        let g = c.acquire(false).unwrap();
        c.prepare(g, 1, b"pay").unwrap();
        assert!(c.prepare(g, 1, b"pay").unwrap_err().contains("unknown"));
    }
}

#[cfg(test)]
mod handoff_tests {
    use super::*;
    #[test]
    fn acceptance_is_atomic_single_use_and_fences_sender() {
        let mut c = Controller::default();
        let old = c.acquire(false).unwrap();
        c.offer_handoff(old, "ticket".into(), 10).unwrap();
        assert!(c.check(old).is_ok());
        assert!(c.accept_handoff("wrong", 11).is_err());
        let new = c.accept_handoff("ticket", 11).unwrap();
        assert!(c.check(old).is_err());
        assert!(c.check(new).is_ok());
        assert!(c.accept_handoff("ticket", 12).is_err());
    }
    #[test]
    fn expired_cancelled_replaced_and_released_offers_cannot_transfer_control() {
        let mut c = Controller::default();
        let old = c.acquire(false).unwrap();
        c.offer_handoff(old, "expired".into(), 10).unwrap();
        assert!(c.accept_handoff("expired", 130).is_err());
        assert!(c.check(old).is_ok());
        c.offer_handoff(old, "cancelled".into(), 131).unwrap();
        c.cancel_handoff(old).unwrap();
        assert!(c.accept_handoff("cancelled", 132).is_err());
        c.offer_handoff(old, "first".into(), 133).unwrap();
        c.offer_handoff(old, "second".into(), 133).unwrap();
        assert!(c.accept_handoff("first", 134).is_err());
        c.release(old).unwrap();
        assert!(c.accept_handoff("second", 135).is_err());
    }
}
