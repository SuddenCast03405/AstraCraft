use std::{collections::VecDeque, time::{Duration, Instant}};

const MAX_SAMPLES: usize = 240;

#[derive(Debug)]
pub struct RollingRate {
    samples: VecDeque<(Instant, u64)>,
    total: u64,
}
impl Default for RollingRate {
    fn default() -> Self { Self { samples: VecDeque::with_capacity(MAX_SAMPLES), total: 0 } }
}
impl RollingRate {
    pub fn push(&mut self, amount: u64) {
        let now = Instant::now();
        self.total = self.total.saturating_add(amount);
        self.samples.push_back((now, amount));
        while self.samples.len() > MAX_SAMPLES { self.samples.pop_front(); }
        self.prune(now);
    }
    fn prune(&mut self, now: Instant) {
        while self.samples.front().is_some_and(|(t, _)| now.duration_since(*t) > Duration::from_secs(10)) {
            self.samples.pop_front();
        }
    }
    pub fn per_second(&mut self) -> f64 {
        let now = Instant::now();
        self.prune(now);
        let Some((first, _)) = self.samples.front() else { return 0.0 };
        let elapsed = now.duration_since(*first).as_secs_f64().max(0.001);
        self.samples.iter().map(|(_, n)| *n as f64).sum::<f64>() / elapsed
    }
    pub fn total(&self) -> u64 { self.total }
}
#[derive(Debug, Default)]
pub struct NetworkMetrics {
    pub packets_in: RollingRate,
    pub packets_out: RollingRate,
    pub bytes_in: RollingRate,
    pub bytes_out: RollingRate,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn total_is_monotonic() {
        let mut r = RollingRate::default();
        r.push(3); r.push(7);
        assert_eq!(r.total(), 10);
    }
}
