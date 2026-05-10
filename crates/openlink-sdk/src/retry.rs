//! # Retry Policy — Smart Retry Strategies
//!
//! Provides configurable retry policies for SDK requests:
//! - **Exponential backoff**: Wait time doubles each retry
//! - **Fixed interval**: Constant wait time between retries
//! - **Custom**: User-defined backoff function
//!
//! ## Example
//!
//! ```rust,ignore
//! use openlink_sdk::retry::{RetryPolicy, RetryCondition};
//!
//! let policy = RetryPolicy::exponential_backoff(3, 100, 10_000);
//! assert_eq!(policy.max_retries(), 3);
//!
//! let condition = RetryCondition::default();
//! assert!(condition.should_retry(500));
//! assert!(!condition.should_retry(401));
//! ```

use std::time::Duration;

/// Retry policy configuration.
#[derive(Debug, Clone)]
pub enum RetryPolicy {
    /// Exponential backoff: wait = initial * multiplier^attempt
    ExponentialBackoff {
        max_retries: u32,
        initial_backoff_ms: u64,
        max_backoff_ms: u64,
        backoff_multiplier: f64,
    },
    /// Fixed interval: constant wait between retries
    FixedInterval {
        max_retries: u32,
        interval_ms: u64,
    },
    /// Custom policy with user-defined parameters
    Custom {
        max_retries: u32,
        delays_ms: Vec<u64>,
    },
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self::ExponentialBackoff {
            max_retries: 0,
            initial_backoff_ms: 100,
            max_backoff_ms: 30_000,
            backoff_multiplier: 2.0,
        }
    }
}

impl RetryPolicy {
    /// Create an exponential backoff policy.
    pub fn exponential_backoff(
        max_retries: u32,
        initial_backoff_ms: u64,
        max_backoff_ms: u64,
    ) -> Self {
        Self::ExponentialBackoff {
            max_retries,
            initial_backoff_ms,
            max_backoff_ms,
            backoff_multiplier: 2.0,
        }
    }

    /// Create a fixed interval policy.
    pub fn fixed_interval(max_retries: u32, interval_ms: u64) -> Self {
        Self::FixedInterval {
            max_retries,
            interval_ms,
        }
    }

    /// Create a custom policy with explicit delay values.
    pub fn custom(max_retries: u32, delays_ms: Vec<u64>) -> Self {
        Self::Custom {
            max_retries,
            delays_ms,
        }
    }

    /// Get the maximum number of retries.
    pub fn max_retries(&self) -> u32 {
        match self {
            Self::ExponentialBackoff { max_retries, .. } => *max_retries,
            Self::FixedInterval { max_retries, .. } => *max_retries,
            Self::Custom { max_retries, .. } => *max_retries,
        }
    }

    /// Get the initial backoff in milliseconds (for exponential).
    pub fn initial_backoff_ms(&self) -> u64 {
        match self {
            Self::ExponentialBackoff { initial_backoff_ms, .. } => *initial_backoff_ms,
            Self::FixedInterval { interval_ms, .. } => *interval_ms,
            Self::Custom { delays_ms, .. } => delays_ms.first().copied().unwrap_or(100),
        }
    }

    /// Get the max backoff in milliseconds.
    pub fn max_backoff_ms(&self) -> u64 {
        match self {
            Self::ExponentialBackoff { max_backoff_ms, .. } => *max_backoff_ms,
            Self::FixedInterval { interval_ms, .. } => *interval_ms,
            Self::Custom { delays_ms, .. } => delays_ms.iter().max().copied().unwrap_or(30_000),
        }
    }

    /// Get the backoff multiplier (for exponential).
    pub fn backoff_multiplier(&self) -> f64 {
        match self {
            Self::ExponentialBackoff { backoff_multiplier, .. } => *backoff_multiplier,
            _ => 1.0,
        }
    }

    /// Calculate the delay for a given attempt (0-based).
    pub fn delay_for_attempt(&self, attempt: u32) -> Duration {
        match self {
            Self::ExponentialBackoff {
                initial_backoff_ms,
                max_backoff_ms,
                backoff_multiplier,
                ..
            } => {
                let delay_ms = (*initial_backoff_ms as f64
                    * backoff_multiplier.powi(attempt as i32))
                    as u64;
                Duration::from_millis(delay_ms.min(*max_backoff_ms))
            }
            Self::FixedInterval { interval_ms, .. } => Duration::from_millis(*interval_ms),
            Self::Custom { delays_ms, .. } => {
                let idx = attempt as usize;
                let delay = delays_ms.get(idx).copied().unwrap_or_else(|| {
                    delays_ms.last().copied().unwrap_or(1000)
                });
                Duration::from_millis(delay)
            }
        }
    }

    /// Check if more retries are available for the given attempt.
    pub fn should_retry(&self, attempt: u32) -> bool {
        attempt < self.max_retries()
    }

    /// Get total maximum retry duration.
    pub fn total_max_duration(&self) -> Duration {
        let mut total_ms: u64 = 0;
        for attempt in 0..self.max_retries() {
            total_ms += self.delay_for_attempt(attempt).as_millis() as u64;
        }
        Duration::from_millis(total_ms)
    }
}

/// Condition for determining whether a request should be retried.
#[derive(Debug, Clone)]
pub struct RetryCondition {
    /// HTTP status codes that should trigger a retry.
    pub retryable_status_codes: Vec<u16>,
    /// Whether to retry on connection errors.
    pub retry_on_connection_error: bool,
    /// Maximum total retry duration.
    pub max_total_duration: Duration,
}

impl Default for RetryCondition {
    fn default() -> Self {
        Self {
            retryable_status_codes: vec![408, 429, 500, 502, 503, 504],
            retry_on_connection_error: true,
            max_total_duration: Duration::from_secs(60),
        }
    }
}

impl RetryCondition {
    /// Create a new retry condition with default settings.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a retry condition that retries on server errors only.
    pub fn server_errors_only() -> Self {
        Self {
            retryable_status_codes: vec![500, 502, 503, 504],
            retry_on_connection_error: true,
            max_total_duration: Duration::from_secs(60),
        }
    }

    /// Create a retry condition that retries on rate limits and server errors.
    pub fn with_rate_limit() -> Self {
        Self {
            retryable_status_codes: vec![429, 500, 502, 503, 504],
            retry_on_connection_error: true,
            max_total_duration: Duration::from_secs(120),
        }
    }

    /// Determine if a request should be retried based on HTTP status code.
    pub fn should_retry(&self, status_code: u16) -> bool {
        self.retryable_status_codes.contains(&status_code)
    }

    /// Determine if a connection error should be retried.
    pub fn should_retry_connection_error(&self) -> bool {
        self.retry_on_connection_error
    }

    /// Check if the total retry duration has been exceeded.
    pub fn is_within_duration(&self, elapsed: Duration) -> bool {
        elapsed < self.max_total_duration
    }

    /// Add a custom status code to the retryable list.
    pub fn with_status_code(mut self, code: u16) -> Self {
        if !self.retryable_status_codes.contains(&code) {
            self.retryable_status_codes.push(code);
        }
        self
    }

    /// Set whether to retry on connection errors.
    pub fn retry_on_connection_error(mut self, retry: bool) -> Self {
        self.retry_on_connection_error = retry;
        self
    }

    /// Set the maximum total retry duration.
    pub fn max_duration(mut self, duration: Duration) -> Self {
        self.max_total_duration = duration;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exponential_backoff() {
        let policy = RetryPolicy::exponential_backoff(3, 100, 10_000);

        assert_eq!(policy.max_retries(), 3);
        assert_eq!(policy.delay_for_attempt(0), Duration::from_millis(100));
        assert_eq!(policy.delay_for_attempt(1), Duration::from_millis(200));
        assert_eq!(policy.delay_for_attempt(2), Duration::from_millis(400));
    }

    #[test]
    fn test_exponential_backoff_capped() {
        let policy = RetryPolicy::exponential_backoff(10, 100, 500);

        // Attempt 10: 100 * 2^10 = 102400, but capped at 500
        assert_eq!(policy.delay_for_attempt(10), Duration::from_millis(500));
    }

    #[test]
    fn test_fixed_interval() {
        let policy = RetryPolicy::fixed_interval(5, 1000);

        assert_eq!(policy.max_retries(), 5);
        assert_eq!(policy.delay_for_attempt(0), Duration::from_millis(1000));
        assert_eq!(policy.delay_for_attempt(4), Duration::from_millis(1000));
    }

    #[test]
    fn test_custom_policy() {
        let policy = RetryPolicy::custom(3, vec![100, 500, 2000]);

        assert_eq!(policy.max_retries(), 3);
        assert_eq!(policy.delay_for_attempt(0), Duration::from_millis(100));
        assert_eq!(policy.delay_for_attempt(1), Duration::from_millis(500));
        assert_eq!(policy.delay_for_attempt(2), Duration::from_millis(2000));
        // Beyond defined delays, use the last value
        assert_eq!(policy.delay_for_attempt(3), Duration::from_millis(2000));
    }

    #[test]
    fn test_should_retry() {
        let policy = RetryPolicy::exponential_backoff(3, 100, 10_000);
        assert!(policy.should_retry(0));
        assert!(policy.should_retry(2));
        assert!(!policy.should_retry(3));
    }

    #[test]
    fn test_total_max_duration() {
        let policy = RetryPolicy::fixed_interval(3, 1000);
        let total = policy.total_max_duration();
        assert_eq!(total, Duration::from_millis(3000));
    }

    #[test]
    fn test_retry_condition_default() {
        let cond = RetryCondition::default();
        assert!(cond.should_retry(500));
        assert!(cond.should_retry(503));
        assert!(cond.should_retry(429));
        assert!(cond.should_retry(408));
        assert!(!cond.should_retry(200));
        assert!(!cond.should_retry(401));
        assert!(!cond.should_retry(404));
    }

    #[test]
    fn test_retry_condition_server_errors_only() {
        let cond = RetryCondition::server_errors_only();
        assert!(cond.should_retry(500));
        assert!(cond.should_retry(502));
        assert!(!cond.should_retry(429)); // Not retryable in server_errors_only
        assert!(!cond.should_retry(408));
    }

    #[test]
    fn test_retry_condition_with_custom_status() {
        let cond = RetryCondition::new().with_status_code(409);
        assert!(cond.should_retry(409));
        assert!(cond.should_retry(500)); // Still has defaults
    }

    #[test]
    fn test_retry_condition_within_duration() {
        let cond = RetryCondition::new().max_duration(Duration::from_secs(30));
        assert!(cond.is_within_duration(Duration::from_secs(10)));
        assert!(!cond.is_within_duration(Duration::from_secs(31)));
    }

    #[test]
    fn test_retry_condition_connection_error() {
        let cond = RetryCondition::new();
        assert!(cond.should_retry_connection_error());

        let cond = cond.retry_on_connection_error(false);
        assert!(!cond.should_retry_connection_error());
    }

    #[test]
    fn test_policy_default() {
        let policy = RetryPolicy::default();
        assert_eq!(policy.max_retries(), 0);
    }

    #[test]
    fn test_exponential_total_duration() {
        let policy = RetryPolicy::exponential_backoff(3, 100, 10_000);
        let total = policy.total_max_duration();
        // 100 + 200 + 400 = 700ms
        assert_eq!(total, Duration::from_millis(700));
    }
}
