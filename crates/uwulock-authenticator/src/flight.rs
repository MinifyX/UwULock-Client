//! The request that is with the person right now, and the flags that end it.
//!
//! A request is known by its [`Flight`] (two shared flags), not only by its
//! channel or transaction id: a browser often sends its next request on the
//! same channel right after an answer, so "the request on channel 7" can
//! already be a new one when the old one's worker finishes. A worker only
//! ever clears its own flight ([`Slot::release`], by identity).
//!
//! - Linux ([`crate::ctaphid`]): the reader [`Slot::replace`]s the slot for
//!   each CBOR request ([`crate::ctaphid::Hid`] already lets only one in),
//!   and the worker hands its answer over with
//!   [`crate::ctaphid::Hid::answered`].
//! - Windows: [`Slot::claim`] refuses a second request while one runs, so
//!   the first can still be cancelled.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

/// One request's flags. Clones share them.
#[derive(Debug, Clone, Default)]
pub struct Flight {
    cancelled: Arc<AtomicBool>,
    resynced: Arc<AtomicBool>,
}

impl Flight {
    /// Set when the request is to stop (CANCEL, re-INIT, Windows' cancel).
    pub fn cancelled(&self) -> &AtomicBool {
        &self.cancelled
    }

    /// Whether its channel was initialised anew meanwhile: then its answer
    /// is stale and isn't sent.
    pub fn resynced(&self) -> bool {
        self.resynced.load(Ordering::Relaxed)
    }

    /// The same request (not just the same channel).
    pub fn same(&self, other: &Flight) -> bool {
        Arc::ptr_eq(&self.cancelled, &other.cancelled)
    }
}

/// At most one request in flight, with the key it came under (a CTAPHID
/// channel, a Windows transaction id).
#[derive(Debug)]
pub struct Slot<K> {
    inner: Mutex<Option<(K, Flight)>>,
}

impl<K> Default for Slot<K> {
    fn default() -> Self {
        Slot::new()
    }
}

impl<K> Slot<K> {
    pub const fn new() -> Self {
        Slot {
            inner: Mutex::new(None),
        }
    }
}

impl<K: PartialEq> Slot<K> {
    fn lock(&self) -> MutexGuard<'_, Option<(K, Flight)>> {
        // A panic elsewhere doesn't make the slot's content wrong.
        self.inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// A new request under `key`, in place of whatever was there.
    pub fn replace(&self, key: K) -> Flight {
        let flight = Flight::default();
        *self.lock() = Some((key, flight.clone()));
        flight
    }

    /// A new request under `key`, only when none is in flight: `None` means
    /// busy, and the request in flight stays as it is.
    pub fn claim(&self, key: K) -> Option<Flight> {
        let mut slot = self.lock();
        if slot.is_some() {
            return None;
        }
        let flight = Flight::default();
        *slot = Some((key, flight.clone()));
        Some(flight)
    }

    /// Cancels the request under `key`, if that is the one in flight;
    /// `resync` also marks its answer stale. Whether there was one.
    pub fn cancel(&self, key: &K, resync: bool) -> bool {
        let slot = self.lock();
        let Some((_, flight)) = slot.as_ref().filter(|(k, _)| k == key) else {
            return false;
        };
        if resync {
            flight.resynced.store(true, Ordering::Relaxed);
        }
        flight.cancelled.store(true, Ordering::Relaxed);
        true
    }

    /// Clears the slot if it still holds `flight`; a newer request stays.
    /// Whether it did.
    pub fn release(&self, flight: &Flight) -> bool {
        let mut slot = self.lock();
        if slot.as_ref().is_some_and(|(_, f)| f.same(flight)) {
            *slot = None;
            return true;
        }
        false
    }

    pub fn is_empty(&self) -> bool {
        self.lock().is_none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_worker_only_clears_its_own_request() {
        let slot = Slot::new();
        let old = slot.replace(7u32);
        // The browser's next request on the same channel, before the old
        // worker cleaned up.
        let new = slot.replace(7);
        assert!(!slot.release(&old));
        assert!(!slot.is_empty());
        // A CANCEL for channel 7 now reaches the new request only.
        assert!(slot.cancel(&7, false));
        assert!(new.cancelled().load(Ordering::Relaxed));
        assert!(!old.cancelled().load(Ordering::Relaxed));
        assert!(slot.release(&new));
        assert!(slot.is_empty());
        assert!(!slot.cancel(&7, false));
    }

    #[test]
    fn resync_marks_the_answer_stale() {
        let slot = Slot::new();
        let flight = slot.replace(3u32);
        assert!(!slot.cancel(&4, true));
        assert!(!flight.resynced());
        assert!(slot.cancel(&3, true));
        assert!(flight.resynced() && flight.cancelled().load(Ordering::Relaxed));
    }

    #[test]
    fn claim_refuses_a_second_request() {
        let slot = Slot::new();
        let first = slot.claim("a").expect("free");
        // A second one is busy, and the first can still be cancelled.
        assert!(slot.claim("b").is_none());
        assert!(slot.cancel(&"a", false));
        assert!(first.cancelled().load(Ordering::Relaxed));
        assert!(slot.release(&first));
        assert!(slot.claim("b").is_some());
    }
}
