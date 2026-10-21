use std::time::{Duration, Instant};

const DEBOUNCE: Duration = Duration::from_millis(200);

/// Ignores toggles that arrive too soon after the previous one (key repeat).
pub struct Debounce {
    last: Option<Instant>,
}

impl Default for Debounce {
    fn default() -> Self {
        Self::new()
    }
}

impl Debounce {
    pub fn new() -> Self {
        Self { last: None }
    }

    pub fn allow(&mut self, now: Instant) -> bool {
        match self.last {
            Some(previous) if now.duration_since(previous) < DEBOUNCE => false,
            _ => {
                self.last = Some(now);
                true
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debounce_blocks_rapid_repeats_but_allows_later_presses() {
        let mut d = Debounce::new();
        let t0 = Instant::now();
        assert!(d.allow(t0));
        assert!(!d.allow(t0 + Duration::from_millis(50)));
        assert!(!d.allow(t0 + Duration::from_millis(199)));
        assert!(d.allow(t0 + Duration::from_millis(250)));
        // The window restarts from the last accepted press.
        assert!(!d.allow(t0 + Duration::from_millis(300)));
    }
}
