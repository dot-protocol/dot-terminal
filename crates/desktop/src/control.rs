//! Who may type into an existing (external) session. Every view starts as an observer: its input and
//! resizes are dropped here, before they reach the owner. One view at a time holds a grant; taking
//! over bumps the session's generation, so the previous holder is fenced on its very next keystroke.
//! A grant lapses after [`IDLE`] without input, so nobody keeps a live agent by accident.
use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant},
};

pub const IDLE: Duration = Duration::from_secs(300);

#[derive(Default)]
pub struct Grants(Mutex<HashMap<(String, String), Grant>>);

struct Grant {
    holder: Option<String>,
    generation: u64,
    last: Instant,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Refusal {
    /// Another view typed recently; it must release, lapse, or be taken over explicitly.
    HeldByAnother,
    /// This view does not hold the grant (never took it, lapsed, or was taken over).
    Observing,
}
impl Refusal {
    pub fn reason(&self) -> &'static str {
        match self {
            Refusal::HeldByAnother => {
                "another view is typing in this session; take over to type here"
            }
            Refusal::Observing => "observing: take control to type into this session",
        }
    }
}

impl Grants {
    fn with<R>(&self, device: &str, session: &str, f: impl FnOnce(&mut Grant) -> R) -> R {
        let mut map = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let g = map
            .entry((device.to_owned(), session.to_owned()))
            .or_insert(Grant {
                holder: None,
                generation: 0,
                last: Instant::now(),
            });
        if g.holder.is_some() && g.last.elapsed() >= IDLE {
            g.holder = None;
        }
        f(g)
    }
    /// Grant `view` control. Without `takeover`, refused while another view holds it.
    pub fn take(
        &self,
        device: &str,
        session: &str,
        view: &str,
        takeover: bool,
    ) -> Result<u64, Refusal> {
        self.with(device, session, |g| {
            match &g.holder {
                Some(h) if h == view => {}
                Some(_) if !takeover => return Err(Refusal::HeldByAnother),
                _ => g.generation += 1,
            }
            g.holder = Some(view.to_owned());
            g.last = Instant::now();
            Ok(g.generation)
        })
    }
    /// Admit one input or resize from `view`; renews its grant.
    pub fn admit(&self, device: &str, session: &str, view: &str) -> Result<(), Refusal> {
        self.with(device, session, |g| match &g.holder {
            Some(h) if h == view => {
                g.last = Instant::now();
                Ok(())
            }
            _ => Err(Refusal::Observing),
        })
    }
    /// Release if `view` holds it. Called on explicit release and whenever the view goes away.
    pub fn release(&self, device: &str, session: &str, view: &str) {
        self.with(device, session, |g| {
            if g.holder.as_deref() == Some(view) {
                g.holder = None;
            }
        })
    }
    #[cfg(test)]
    pub fn holder(&self, device: &str, session: &str) -> Option<String> {
        self.with(device, session, |g| g.holder.clone())
    }
    #[cfg(test)]
    fn age(&self, device: &str, session: &str, by: Duration) {
        self.with(device, session, |g| g.last -= by)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn views_observe_until_they_take_control() {
        let g = Grants::default();
        assert_eq!(g.admit("vps", "s_1", "a"), Err(Refusal::Observing));
        g.take("vps", "s_1", "a", false).unwrap();
        assert_eq!(g.admit("vps", "s_1", "a"), Ok(()));
        assert_eq!(g.admit("vps", "s_1", "b"), Err(Refusal::Observing));
        assert_eq!(
            g.admit("vps", "s_2", "a"),
            Err(Refusal::Observing),
            "a grant is per session"
        );
    }
    #[test]
    fn takeover_is_explicit_and_fences_the_previous_holder() {
        let g = Grants::default();
        let first = g.take("vps", "s_1", "a", false).unwrap();
        assert_eq!(
            g.take("vps", "s_1", "b", false),
            Err(Refusal::HeldByAnother)
        );
        let second = g.take("vps", "s_1", "b", true).unwrap();
        assert!(second > first);
        assert_eq!(
            g.admit("vps", "s_1", "a"),
            Err(Refusal::Observing),
            "old holder fenced at once"
        );
        assert_eq!(g.admit("vps", "s_1", "b"), Ok(()));
        assert_eq!(
            g.take("vps", "s_1", "b", false),
            Ok(second),
            "re-taking your own grant is not a takeover"
        );
    }
    #[test]
    fn an_idle_grant_lapses_and_release_frees_it() {
        let g = Grants::default();
        g.take("vps", "s_1", "a", false).unwrap();
        g.age("vps", "s_1", IDLE);
        assert_eq!(
            g.admit("vps", "s_1", "a"),
            Err(Refusal::Observing),
            "lapsed after idle"
        );
        assert!(
            g.take("vps", "s_1", "b", false).is_ok(),
            "a lapsed grant needs no takeover"
        );
        g.release("vps", "s_1", "a");
        assert_eq!(
            g.holder("vps", "s_1").as_deref(),
            Some("b"),
            "only the holder can release"
        );
        g.release("vps", "s_1", "b");
        assert_eq!(g.holder("vps", "s_1"), None);
    }
}
