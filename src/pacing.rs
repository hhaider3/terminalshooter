use std::time::{Duration, Instant};

pub struct FramePacer {
    deadline: Instant,
    interval: Duration,
}

impl FramePacer {
    pub fn new(start: Instant, interval: Duration) -> Self {
        Self {
            deadline: start,
            interval,
        }
    }

    fn advance(&mut self, now: Instant) -> Instant {
        // Keep deadlines on the original cadence. OS sleep overshoot must not
        // add to every following frame's duration.
        self.deadline += self.interval;
        if now.saturating_duration_since(self.deadline) >= self.interval {
            // Output stalls should not trigger a burst of catch-up frames.
            self.deadline = now;
        }
        self.deadline
    }

    pub fn wait(&mut self) {
        let now = Instant::now();
        let deadline = self.advance(now);
        let remaining = deadline.saturating_duration_since(Instant::now());
        if !remaining.is_zero() {
            std::thread::sleep(remaining);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn late_wakeups_do_not_accumulate_into_a_lower_frame_rate() {
        let start = Instant::now();
        let interval = Duration::from_secs_f64(1.0 / 60.0);
        let mut pacer = FramePacer::new(start, interval);
        let mut now = start + Duration::from_millis(1);
        for frame in 1..=600 {
            let deadline = pacer.advance(now);
            assert_eq!(deadline, start + interval * frame);
            // Simulate a 3ms late wakeup and 1ms of rendering each frame.
            now = deadline + Duration::from_millis(4);
        }
    }

    #[test]
    fn long_stall_resets_cadence_without_catchup_burst() {
        let start = Instant::now();
        let interval = Duration::from_secs_f64(1.0 / 60.0);
        let mut pacer = FramePacer::new(start, interval);
        let stalled = start + Duration::from_secs(1);
        assert_eq!(pacer.advance(stalled), stalled);
        assert_eq!(
            pacer.advance(stalled + Duration::from_millis(1)),
            stalled + interval
        );
    }
}
