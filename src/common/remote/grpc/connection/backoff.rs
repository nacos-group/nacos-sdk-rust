use std::time::Duration;

/// Jittered exponential backoff used when re-establishing a broken connection.
///
/// Delays use "equal jitter": the delay for an attempt is drawn uniformly from
/// `[full / 2, full]`, so consecutive failures keep a lower bound while still
/// spreading reconnects out to avoid a thundering herd.
#[derive(Clone, Debug)]
pub(crate) struct RetryPolicy {
    base: Duration,
    factor: u32,
    cap: Duration,
    stable_reset_after: Duration,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            base: Duration::from_millis(500),
            factor: 2,
            cap: Duration::from_secs(30),
            stable_reset_after: Duration::from_secs(10),
        }
    }
}

impl RetryPolicy {
    /// How long a connection has to stay healthy before the attempt counter and
    /// the backoff delay are reset.
    pub(crate) fn stable_reset_after(&self) -> Duration {
        self.stable_reset_after
    }

    /// Randomised delay for `attempt` (0-based).
    pub(crate) fn delay(&self, attempt: u32) -> Duration {
        let full = self.full_delay(attempt);
        let half_ms = (full / 2).as_millis() as u64;
        if half_ms == 0 {
            return full;
        }
        self.delay_with_roll(attempt, rand::random_range(0..=half_ms))
    }

    /// Deterministic variant of [`RetryPolicy::delay`] for tests: `roll` is the
    /// raw random value that would have been drawn.
    pub(crate) fn delay_with_roll(&self, attempt: u32, roll: u64) -> Duration {
        let full = self.full_delay(attempt);
        let half = full / 2;
        let half_ms = half.as_millis() as u64;
        if half_ms == 0 {
            return full;
        }
        let extra_ms = roll % (half_ms + 1);
        half + Duration::from_millis(extra_ms)
    }

    fn full_delay(&self, attempt: u32) -> Duration {
        // Clamping the exponent keeps `saturating_pow` cheap and overflow-free
        // for absurd attempt counters.
        let exponent = attempt.min(32);
        let multiplier = self.factor.saturating_pow(exponent);
        self.base.saturating_mul(multiplier).min(self.cap)
    }

    #[cfg(test)]
    pub(crate) fn for_test(
        base_ms: u64,
        factor: u32,
        cap_ms: u64,
        stable_reset_after_ms: u64,
    ) -> Self {
        Self {
            base: Duration::from_millis(base_ms),
            factor,
            cap: Duration::from_millis(cap_ms),
            stable_reset_after: Duration::from_millis(stable_reset_after_ms),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_delay_is_between_half_and_full() {
        let policy = RetryPolicy::default();
        for attempt in 0..40 {
            let full = 500u64
                .saturating_mul(2u64.saturating_pow(attempt.min(32)))
                .min(30_000);
            let half = full / 2;
            assert_eq!(policy.delay_with_roll(attempt, 0).as_millis() as u64, half);
            assert_eq!(
                policy.delay_with_roll(attempt, half).as_millis() as u64,
                full
            );
            for roll in [0, 1, half / 2, half, half + 1] {
                let delay = policy.delay_with_roll(attempt, roll).as_millis() as u64;
                assert!(
                    delay >= half,
                    "attempt {attempt} roll {roll}: {delay} < {half}"
                );
                assert!(
                    delay <= full,
                    "attempt {attempt} roll {roll}: {delay} > {full}"
                );
            }
        }
    }

    #[test]
    fn test_attempt_overflow_is_safe() {
        let policy = RetryPolicy::default();
        let delay = policy.delay(u32::MAX);
        assert!(delay <= Duration::from_secs(30));
    }

    #[test]
    fn test_stable_reset_after_is_ten_seconds() {
        assert_eq!(
            RetryPolicy::default().stable_reset_after(),
            Duration::from_secs(10)
        );
    }
}
