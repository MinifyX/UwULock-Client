//! Copying secrets, from Rust: the value goes straight from the vault to the
//! clipboard and never through the page.
//!
//! The host copies ([`Host::clipboard_copy`]): on Windows the copy is kept out
//! of the clipboard history (Win+V) and cloud clipboard, on macOS out of
//! clipboard managers that honour the concealed type; Android marks the clip
//! as sensitive, iOS keeps it on the device and lets it expire by itself at
//! the chosen time. After the chosen time the clipboard is cleared — but only
//! if it still holds what UwULock put there, so something copied since stays.

use parking_lot::Mutex;
use sha2::{Digest, Sha256};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use crate::host::Host;

#[derive(Default)]
pub struct Clipboard {
    /// The hash of the last value copied and a counter, so an older timer
    /// never clears a newer copy.
    last: Mutex<Option<(u64, [u8; 32])>>,
    counter: Mutex<u64>,
    /// Set once by [`crate::App::new`].
    host: OnceLock<Arc<dyn Host>>,
}

fn digest(text: &str) -> [u8; 32] {
    Sha256::digest(text.as_bytes()).into()
}

impl Clipboard {
    pub(crate) fn attach(&self, host: Arc<dyn Host>) {
        let _ = self.host.set(host);
    }

    fn host(&self) -> Result<&Arc<dyn Host>, String> {
        self.host
            .get()
            .ok_or_else(|| "No clipboard yet.".to_string())
    }

    pub fn copy(self: &Arc<Self>, text: &str, clear_after: Option<Duration>) -> Result<(), String> {
        self.host()?.clipboard_copy(text, true, clear_after)?;

        let generation = {
            let mut counter = self.counter.lock();
            *counter += 1;
            *counter
        };
        *self.last.lock() = Some((generation, digest(text)));

        if let Some(after) = clear_after {
            let this = self.clone();
            std::thread::spawn(move || {
                std::thread::sleep(after);
                this.clear_if_ours(generation);
            });
        }
        Ok(())
    }

    /// Text that isn't secret (the AutoFill protocol): an ordinary copy
    /// that may go to other devices, never cleared.
    pub fn copy_plain(&self, text: &str) -> Result<(), String> {
        self.host()?.clipboard_copy(text, false, None)
    }

    fn clear_if_ours(&self, generation: u64) {
        // Checked and taken under one guard: a copy in between keeps its own.
        let hash = {
            let mut last = self.last.lock();
            match *last {
                Some((latest, hash)) if latest == generation => {
                    *last = None;
                    hash
                }
                _ => return,
            }
        };
        if let Ok(host) = self.host() {
            host.clipboard_clear(&hash);
        }
    }

    /// Locking clears a copied secret right away.
    pub fn clear_now(self: &Arc<Self>) {
        let generation = self.last.lock().map(|(g, _)| g);
        if let Some(generation) = generation {
            self.clear_if_ours(generation);
        }
    }
}
