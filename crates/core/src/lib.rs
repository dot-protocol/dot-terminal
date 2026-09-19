//! Pure state machines: no filesystem, networking, wall clock, or model backend.
use std::collections::VecDeque;

pub struct History {
    bytes: VecDeque<u8>,
    capacity: usize,
    next: u64,
    /// Grid sizes in stream order: `(first offset it applies to, geometry)`. Never empty.
    marks: VecDeque<(u64, Geometry)>,
}
/// The PTY grid a run of output bytes was produced under. `epoch` counts resizes of this
/// stream, so two views can tell "same size again" from "nothing changed".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Geometry {
    pub cols: u16,
    pub rows: u16,
    pub epoch: u64,
}
pub struct Chunk {
    pub start: u64,
    pub next: u64,
    pub gap: bool,
    pub data: Vec<u8>,
}
/// A chunk that never spans a resize: every byte in it belongs to `geometry`.
pub struct Frame {
    pub chunk: Chunk,
    pub geometry: Geometry,
}
/// Resizes remembered per stream. A drag produces dozens; older marks than the retained
/// bytes are useless, and a flood of resizes must not grow without bound.
const MAX_MARKS: usize = 256;
impl History {
    pub fn new(capacity: usize) -> Self {
        Self::with_geometry(capacity, 80, 24)
    }
    /// A history whose stream starts on a `cols` x `rows` grid (epoch 0).
    pub fn with_geometry(capacity: usize, cols: u16, rows: u16) -> Self {
        assert!(capacity > 0);
        let start = Geometry {
            cols,
            rows,
            epoch: 0,
        };
        Self {
            bytes: VecDeque::new(),
            capacity,
            next: 0,
            marks: VecDeque::from([(0, start)]),
        }
    }
    /// The grid changes HERE in the stream: every byte appended from now on belongs to it.
    /// Call this at the moment the PTY is resized, while no append can interleave.
    pub fn resize(&mut self, cols: u16, rows: u16) -> Geometry {
        let epoch = self
            .geometry()
            .epoch
            .checked_add(1)
            .expect("resize epoch exhausted");
        let geometry = Geometry { cols, rows, epoch };
        // Two resizes with no output between them: only the last one ever applied to a byte.
        if self.marks.back().is_some_and(|(at, _)| *at == self.next) {
            self.marks.pop_back();
        }
        self.marks.push_back((self.next, geometry));
        self.prune();
        geometry
    }
    /// The grid in force at the head of the stream.
    pub fn geometry(&self) -> Geometry {
        self.marks.back().expect("marks is never empty").1
    }
    fn oldest(&self) -> u64 {
        self.next - self.bytes.len() as u64
    }
    /// Keep the mark in force at the oldest retained byte and everything after it, bounded.
    fn prune(&mut self) {
        let oldest = self.oldest();
        while self.marks.len() > 1 && self.marks[1].0 <= oldest {
            self.marks.pop_front();
        }
        while self.marks.len() > MAX_MARKS {
            self.marks.pop_front();
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
        self.prune();
    }
    /// Like [`History::read`], but the chunk stops at the next resize, so the whole of it
    /// was produced under the returned geometry. A reader that applies `geometry` before
    /// parsing `chunk` sees bytes and grid in the order the PTY produced them.
    pub fn read_frame(&self, after: u64, limit: usize) -> Result<Frame, &'static str> {
        if after > self.next {
            return Err("cursor is ahead of session output");
        }
        let start = after.max(self.oldest());
        // Last mark at or before `start`. If marks were pruned by the bound, the earliest
        // one kept is the best statement available.
        let at = self
            .marks
            .iter()
            .rposition(|(offset, _)| *offset <= start)
            .unwrap_or(0);
        let until = self
            .marks
            .get(at + 1)
            .map_or(u64::MAX, |(offset, _)| *offset);
        let room = usize::try_from(until - start).unwrap_or(usize::MAX);
        Ok(Frame {
            chunk: self.read(after, limit.min(room))?,
            geometry: self.marks[at].1,
        })
    }
    pub fn read(&self, after: u64, limit: usize) -> Result<Chunk, &'static str> {
        if after > self.next {
            return Err("cursor is ahead of session output");
        }
        let oldest = self.oldest();
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
    fn a_frame_never_spans_a_resize_and_names_the_grid_its_bytes_were_made_under() {
        let mut h = History::with_geometry(64, 80, 24);
        h.append(b"aaaa");
        let wide = h.resize(120, 40);
        h.append(b"bbbb");
        h.resize(100, 30);
        let narrow = h.resize(60, 20); // two resizes, no output between: only the last counts
        h.append(b"cc");
        assert_eq!((wide.epoch, narrow.epoch), (1, 3));

        let f = h.read_frame(0, 100).unwrap();
        assert_eq!(
            (f.chunk.data, f.chunk.next),
            (b"aaaa".to_vec(), 4),
            "stops at the resize"
        );
        assert_eq!(
            (f.geometry.cols, f.geometry.rows, f.geometry.epoch),
            (80, 24, 0)
        );
        let f = h.read_frame(4, 100).unwrap();
        assert_eq!((f.chunk.data, f.geometry), (b"bbbb".to_vec(), wide));
        let f = h.read_frame(6, 100).unwrap();
        assert_eq!(
            (f.chunk.data, f.geometry),
            (b"bb".to_vec(), wide),
            "mid-run cursor"
        );
        let f = h.read_frame(8, 1).unwrap();
        assert_eq!(
            (f.chunk.data, f.geometry),
            (b"c".to_vec(), narrow),
            "limit still applies"
        );
        // caught up: nothing to read, and the grid is the current one
        let f = h.read_frame(10, 100).unwrap();
        assert!(f.chunk.data.is_empty() && !f.chunk.gap);
        assert_eq!((f.geometry, h.geometry()), (narrow, narrow));
        assert!(h.read_frame(11, 1).is_err());
        // a resize with nothing after it yet is still reported to a caught-up reader
        let last = h.resize(90, 28);
        assert_eq!(h.read_frame(10, 100).unwrap().geometry, last);
        // plain reads are unchanged: they cross resizes as they always did
        assert_eq!(h.read(0, 100).unwrap().data, b"aaaabbbbcc");
    }
    #[test]
    fn marks_follow_the_ring_and_stay_bounded() {
        let mut h = History::with_geometry(4, 80, 24);
        h.append(b"ab");
        let g = h.resize(100, 30);
        h.append(b"cdefgh"); // ring keeps "efgh"; the 80x24 bytes are gone
        let f = h.read_frame(0, 100).unwrap();
        assert!(f.chunk.gap);
        assert_eq!(
            (f.chunk.start, f.chunk.data, f.geometry),
            (4, b"efgh".to_vec(), g)
        );
        assert_eq!(
            h.marks.len(),
            1,
            "a mark older than every retained byte is dropped"
        );

        let mut h = History::with_geometry(1 << 20, 80, 24);
        for i in 0..1000u16 {
            h.append(b"x");
            h.resize(80 + i % 50, 24);
        }
        assert!(h.marks.len() <= MAX_MARKS);
        assert_eq!(
            h.geometry().epoch,
            1000,
            "epochs keep counting past the bound"
        );
        // a reader far behind still gets bytes, labelled with the earliest grid still known
        let f = h.read_frame(0, 10).unwrap();
        assert_eq!(f.chunk.start, 0);
        assert!(!f.chunk.data.is_empty() && f.geometry.epoch > 0);
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
