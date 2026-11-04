//! Optional regular-file allocation observation, never a free-space estimate.
pub use files_core::folder_usage::{MAX_IDENTITIES, Observation, Reason, Value};
use std::collections::BTreeMap;

fn add(value: &mut Value, bytes: Option<u64>) {
    if let Value::Bytes(total) = *value {
        *value = match bytes.and_then(|n| total.checked_add(n)) {
            Some(n) => Value::Bytes(n),
            None => Value::Unavailable(Reason::Overflow),
        };
    }
}

pub(super) struct Accumulator {
    observation: Observation,
    identities: BTreeMap<(u64, u64), u64>,
    limit: usize,
}
impl Default for Accumulator {
    fn default() -> Self {
        Self::new(MAX_IDENTITIES)
    }
}
impl Accumulator {
    fn new(limit: usize) -> Self {
        Self {
            observation: Observation {
                by_name: Value::Bytes(0),
                by_identity: Value::Bytes(0),
            },
            identities: BTreeMap::new(),
            limit,
        }
    }
    pub(super) fn observe(&mut self, device: u64, inode: u64, blocks: u64) {
        let bytes = blocks.checked_mul(512);
        add(&mut self.observation.by_name, bytes);
        if !matches!(self.observation.by_identity, Value::Bytes(_)) {
            return;
        }
        let key = (device, inode);
        match self.identities.get(&key) {
            Some(previous) if *previous != blocks => {
                self.observation.by_identity = Value::Unavailable(Reason::ChangedIdentity);
            }
            Some(_) => return,
            None if self.identities.len() >= self.limit => {
                self.observation.by_identity = Value::Unavailable(Reason::IdentityLimit);
            }
            None => {
                add(&mut self.observation.by_identity, bytes);
                if matches!(self.observation.by_identity, Value::Bytes(_)) {
                    self.identities.insert(key, blocks);
                }
            }
        }
        if !matches!(self.observation.by_identity, Value::Bytes(_)) {
            self.identities.clear();
        }
    }
    pub(super) fn finish(self) -> Observation {
        self.observation
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn aliases_count_by_name_but_identity_includes_device() {
        let mut a = Accumulator::new(2);
        a.observe(1, 7, 8);
        a.observe(1, 7, 8);
        a.observe(2, 7, 8);
        assert_eq!(
            a.finish(),
            Observation {
                by_name: Value::Bytes(12288),
                by_identity: Value::Bytes(8192)
            }
        );
    }
    #[test]
    fn limit_never_turns_into_partial_or_zero_and_names_continue() {
        let mut a = Accumulator::new(1);
        a.observe(1, 1, 0);
        a.observe(1, 1, 0);
        assert_eq!(a.identities.len(), 1);
        a.observe(1, 2, 8);
        a.observe(1, 3, 16);
        assert!(a.identities.is_empty());
        assert_eq!(
            a.finish(),
            Observation {
                by_name: Value::Bytes(12288),
                by_identity: Value::Unavailable(Reason::IdentityLimit)
            }
        );
    }
    #[test]
    fn conflicting_alias_is_explicit_and_sticky() {
        let mut a = Accumulator::default();
        a.observe(1, 1, 8);
        a.observe(1, 1, 16);
        a.observe(1, 1, 8);
        assert_eq!(
            a.finish(),
            Observation {
                by_name: Value::Bytes(16384),
                by_identity: Value::Unavailable(Reason::ChangedIdentity)
            }
        );
    }
    #[test]
    fn multiply_and_sum_overflow_are_unavailable() {
        let mut a = Accumulator::default();
        a.observe(1, 1, u64::MAX);
        a.observe(1, 2, 0);
        assert_eq!(
            a.finish(),
            Observation {
                by_name: Value::Unavailable(Reason::Overflow),
                by_identity: Value::Unavailable(Reason::Overflow)
            }
        );
        let mut a = Accumulator::default();
        a.observe(1, 1, u64::MAX / 512);
        a.observe(1, 2, 1);
        assert_eq!(a.finish().by_identity, Value::Unavailable(Reason::Overflow));
    }

    #[test]
    fn alias_sum_overflow_does_not_invalidate_unique_observation() {
        let mut a = Accumulator::default();
        let blocks = u64::MAX / 512;
        a.observe(1, 1, blocks);
        a.observe(1, 1, blocks);
        assert_eq!(
            a.finish(),
            Observation {
                by_name: Value::Unavailable(Reason::Overflow),
                by_identity: Value::Bytes(blocks * 512),
            }
        );
    }
}
