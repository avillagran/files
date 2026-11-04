//! FL_PASTE carries no request ID. An invalidated request keeps its slot until
//! callback drain. Caller drops Model's RefMut BEFORE calling paste_text().
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompletionContract {
    /// X11/macOS/Windows/unknown: callback may still arrive after return.
    MayBeAsynchronous,
    /// Only pinned FLTK1.5.23's actual Wayland backend. Own AND external text
    /// callbacks are synchronous; return without one is definitive NoCallback.
    PinnedWaylandSynchronous,
}
/// Identifies OUR invocation/return, never misrepresented as a FL_PASTE ID.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Receipt(u64);
#[derive(Debug)]
struct Pending<K> {
    key: K,
    valid: bool,
    receipt: Receipt,
}
#[derive(Debug)]
pub struct Gate<K> {
    pending: Option<Pending<K>>,
    next: u64,
}
impl<K> Default for Gate<K> {
    fn default() -> Self {
        Self {
            pending: None,
            next: 0,
        }
    }
}
impl<K: PartialEq> Gate<K> {
    pub fn request(&mut self, key: K) -> Option<Receipt> {
        if self.pending.is_some() {
            return None;
        }
        self.next = self
            .next
            .checked_add(1)
            .expect("paste invocation ID exhausted");
        let receipt = Receipt(self.next);
        self.pending = Some(Pending {
            key,
            valid: true,
            receipt,
        });
        Some(receipt)
    }
    pub fn request_if_compatible(&mut self, key: K, compatible: bool) -> Option<Receipt> {
        if compatible { self.request(key) } else { None }
    }
    pub fn returned(&mut self, receipt: Receipt, contract: CompletionContract) {
        if contract == CompletionContract::PinnedWaylandSynchronous
            && self.pending.as_ref().is_some_and(|p| p.receipt == receipt)
        {
            self.pending = None;
        }
    }
    pub fn invalidate(&mut self) {
        if let Some(p) = &mut self.pending {
            p.valid = false;
        }
    }
    pub fn receive(&mut self, current: &K) -> bool {
        self.pending
            .take()
            .is_some_and(|p| p.valid && p.key == *current)
    }
    pub fn pending(&self) -> bool {
        self.pending.is_some()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn invalidated_a_keeps_slot_until_callback_then_b_can_receive() {
        let mut g = Gate::default();
        assert!(g.request((1, "omnibar")).is_some());
        g.invalidate();
        assert!(g.pending());
        assert!(g.request((2, "rename")).is_none());
        assert!(!g.receive(&(2, "rename")));
        assert!(!g.pending());
        assert!(g.request((2, "rename")).is_some());
        assert!(g.receive(&(2, "rename")));
        assert!(!g.receive(&(2, "rename")));
    }
    #[test]
    fn same_key_reentered_does_not_revive_a_tombstone() {
        let mut g = Gate::default();
        assert!(g.request(1).is_some());
        g.invalidate();
        assert!(g.request(1).is_none());
        assert!(!g.receive(&1));
        assert!(g.request(1).is_some());
        assert!(g.receive(&1));
    }
    #[test]
    fn double_request_preserves_first_and_context_mismatch_drops() {
        let mut g = Gate::default();
        assert!(g.request(1).is_some());
        assert!(g.request(2).is_none());
        assert!(!g.receive(&2));
        assert!(g.request(3).is_some());
        assert!(g.receive(&3));
        assert!(!g.pending());
    }
    #[test]
    fn unavailable_or_synchronous_no_callback_does_not_poison_later_text() {
        let mut g = Gate::default();
        assert!(g.request_if_compatible(1, false).is_none());
        assert!(!g.pending());
        // MIME may disappear between preflight and paste. Definitive return
        // avoids a stuck slot without timeout-based reassignment.
        let a = g.request_if_compatible(2, true).unwrap();
        g.returned(a, CompletionContract::PinnedWaylandSynchronous);
        assert!(!g.pending());
        let b = g.request_if_compatible(3, true).unwrap();
        assert!(g.receive(&3));
        g.returned(b, CompletionContract::PinnedWaylandSynchronous);
        assert!(!g.pending());
    }
    #[test]
    fn async_return_preserves_tombstone_without_timeout_reassignment() {
        let mut g = Gate::default();
        let a = g.request(1).unwrap();
        g.invalidate();
        g.returned(a, CompletionContract::MayBeAsynchronous);
        assert!(g.pending());
        assert!(g.request(2).is_none());
        assert!(!g.receive(&2));
        assert!(g.request(2).is_some());
        assert!(g.receive(&2));
    }
    #[test]
    fn synchronous_callback_can_borrow_receiver_after_request_borrow_is_released() {
        let gate = std::cell::RefCell::new(Gate::default());
        let receipt = {
            let mut receiver = gate.borrow_mut();
            receiver.request(1).unwrap()
        };
        let simulated_callback = || gate.borrow_mut().receive(&1);
        assert!(simulated_callback());
        gate.borrow_mut()
            .returned(receipt, CompletionContract::PinnedWaylandSynchronous);
        assert!(!gate.borrow().pending());
    }
    #[test]
    fn old_return_does_not_clear_reentrant_new_request_after_callback_drain() {
        let mut g = Gate::default();
        let a = g.request(1).unwrap();
        assert!(g.receive(&1));
        let b = g.request(2).unwrap();
        g.returned(a, CompletionContract::PinnedWaylandSynchronous);
        assert!(g.pending());
        assert!(g.receive(&2));
        g.returned(b, CompletionContract::PinnedWaylandSynchronous);
    }
}
