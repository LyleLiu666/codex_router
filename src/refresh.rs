use std::time::{Duration, Instant};

#[derive(Debug, Default, Clone)]
pub struct RefreshSchedule {
    next_due: Option<Instant>,
}

impl RefreshSchedule {
    pub fn new() -> Self {
        Self { next_due: None }
    }

    pub fn clear(&mut self) {
        self.next_due = None;
    }

    pub fn next_due(&self) -> Option<Instant> {
        self.next_due
    }

    pub fn tick(&mut self, now: Instant, interval: Duration) -> bool {
        match self.next_due {
            None => {
                self.next_due = Some(now + interval);
                false
            }
            Some(due) if now >= due => {
                self.next_due = Some(now + interval);
                true
            }
            Some(_) => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schedules_next_when_missing() {
        let mut schedule = RefreshSchedule::new();
        let now = Instant::now();
        let interval = Duration::from_secs(60);

        let triggered = schedule.tick(now, interval);

        assert!(!triggered);
        assert_eq!(schedule.next_due(), Some(now + interval));
    }

    #[test]
    fn triggers_when_due_and_reschedules() {
        let mut schedule = RefreshSchedule::new();
        let interval = Duration::from_secs(60);
        let now = Instant::now();
        schedule.next_due = Some(now - Duration::from_secs(1));

        let triggered = schedule.tick(now, interval);

        assert!(triggered);
        assert_eq!(schedule.next_due(), Some(now + interval));
    }

    #[test]
    fn does_not_trigger_before_due() {
        let mut schedule = RefreshSchedule::new();
        let interval = Duration::from_secs(60);
        let now = Instant::now();
        schedule.next_due = Some(now + Duration::from_secs(10));

        let triggered = schedule.tick(now, interval);

        assert!(!triggered);
        assert_eq!(schedule.next_due(), Some(now + Duration::from_secs(10)));
    }

    #[test]
    fn clear_resets_schedule() {
        let mut schedule = RefreshSchedule::new();
        schedule.next_due = Some(Instant::now());

        schedule.clear();

        assert!(schedule.next_due().is_none());
    }
}
