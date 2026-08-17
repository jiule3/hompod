use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LatencyProfile {
    Ultra,
    Low,
    Stable,
}

impl Default for LatencyProfile {
    fn default() -> Self {
        Self::Ultra
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LatencyTuning {
    pub sender_lead_ms: u32,
    pub pcm_queue_frames: usize,
    pub reconnect_grace_ms: u64,
}

impl LatencyProfile {
    pub const fn tuning(self) -> LatencyTuning {
        match self {
            Self::Ultra => LatencyTuning {
                sender_lead_ms: 250,
                pcm_queue_frames: 8,
                reconnect_grace_ms: 150,
            },
            Self::Low => LatencyTuning {
                sender_lead_ms: 350,
                pcm_queue_frames: 24,
                reconnect_grace_ms: 300,
            },
            Self::Stable => LatencyTuning {
                sender_lead_ms: 500,
                pcm_queue_frames: 64,
                reconnect_grace_ms: 500,
            },
        }
    }

    pub const fn fallback(self) -> Self {
        match self {
            LatencyProfile::Ultra => LatencyProfile::Low,
            LatencyProfile::Low => LatencyProfile::Stable,
            LatencyProfile::Stable => LatencyProfile::Stable,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Receiver {
    pub id: String,
    pub name: String,
    pub address: String,
    pub port: u16,
    pub model: Option<String>,
    pub is_stereo_pair: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BridgeState {
    Idle,
    Discovering,
    Connecting { receiver_id: String },
    Streaming { receiver_id: String, profile: LatencyProfile },
    Reconnecting { receiver_id: String, attempt: u8, profile: LatencyProfile },
    Error { message: String },
}

#[derive(Debug, Clone)]
pub struct StabilityPolicy {
    pub fallback_after_failures: u8,
    pub max_reconnect_attempts: u8,
}

impl Default for StabilityPolicy {
    fn default() -> Self {
        Self {
            fallback_after_failures: 2,
            max_reconnect_attempts: 5,
        }
    }
}

impl StabilityPolicy {
    pub fn profile_for_attempt(&self, requested: LatencyProfile, failures: u8) -> LatencyProfile {
        // Ultra means latency-first. Never silently turn it into Low/Stable.
        // We still reconnect and eventually give up after max_reconnect_attempts,
        // but while connected the receiver latency request remains Gaming/Ultra.
        if requested == LatencyProfile::Ultra {
            return LatencyProfile::Ultra;
        }

        if failures < self.fallback_after_failures {
            return requested;
        }
        let first = requested.fallback();
        if failures < self.fallback_after_failures.saturating_mul(2) {
            first
        } else {
            first.fallback()
        }
    }

    pub fn reconnect_delay_ms(&self, attempt: u8) -> u64 {
        let shift = attempt.min(4) as u32;
        250_u64.saturating_mul(1_u64 << shift).min(4_000)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ultra_is_default_and_targets_250ms_receiver_floor() {
        assert_eq!(LatencyProfile::default(), LatencyProfile::Ultra);
        assert_eq!(LatencyProfile::Ultra.tuning().sender_lead_ms, 250);
    }

    #[test]
    fn ultra_never_silently_degrades() {
        let p = StabilityPolicy::default();
        for failures in 0..=10 {
            assert_eq!(
                p.profile_for_attempt(LatencyProfile::Ultra, failures),
                LatencyProfile::Ultra
            );
        }
    }

    #[test]
    fn low_can_still_fallback_to_stable() {
        let p = StabilityPolicy::default();
        assert_eq!(
            p.profile_for_attempt(LatencyProfile::Low, p.fallback_after_failures),
            LatencyProfile::Stable
        );
    }

    #[test]
    fn reconnect_backoff_caps_at_four_seconds() {
        let p = StabilityPolicy::default();
        assert_eq!(p.reconnect_delay_ms(0), 250);
        assert_eq!(p.reconnect_delay_ms(4), 4_000);
        assert_eq!(p.reconnect_delay_ms(9), 4_000);
    }
}
