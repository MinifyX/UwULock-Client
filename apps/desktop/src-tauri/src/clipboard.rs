//! Copying secrets, from Rust: the value goes straight from the vault to the
//! clipboard and never through the page.
//!
//! On Windows the copy is kept out of the clipboard history (Win+V) and cloud
//! clipboard, on macOS out of clipboard managers that honour the concealed
//! type. After the chosen time the clipboard is cleared — but only if it still
//! holds what UwULock put there, so something copied since stays.

use parking_lot::Mutex;
use sha2::{Digest, Sha256};
use std::sync::Arc;
use std::time::Duration;

#[derive(Default)]
pub struct Clipboard {
    /// The hash of the last value copied and a counter, so an older timer
    /// never clears a newer copy.
    last: Mutex<Option<(u64, [u8; 32])>>,
    counter: Mutex<u64>,
}

fn digest(text: &str) -> [u8; 32] {
    Sha256::digest(text.as_bytes()).into()
}

impl Clipboard {
    pub fn copy(self: &Arc<Self>, text: &str, clear_after: Option<Duration>) -> Result<(), String> {
        let mut clipboard = arboard::Clipboard::new().map_err(|e| format!("No clipboard: {e}"))?;
        let set = clipboard.set();
        #[cfg(windows)]
        let set = {
            use arboard::SetExtWindows;
            set.exclude_from_history().exclude_from_cloud()
        };
        #[cfg(target_os = "macos")]
        let set = {
            use arboard::SetExtApple;
            set.exclude_from_history()
        };
        #[cfg(all(unix, not(target_os = "macos")))]
        let set = {
            use arboard::SetExtLinux;
            set.exclude_from_history()
        };
        set.text(text.to_string())
            .map_err(|e| format!("Couldn't copy: {e}"))?;

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

    fn clear_if_ours(&self, generation: u64) {
        let Some((latest, hash)) = *self.last.lock() else {
            return;
        };
        if latest != generation {
            return;
        }
        let Ok(mut clipboard) = arboard::Clipboard::new() else {
            return;
        };
        if clipboard
            .get_text()
            .is_ok_and(|current| digest(&current) == hash)
        {
            let _ = clipboard.clear();
            tracing::debug!("clipboard cleared");
        }
        *self.last.lock() = None;
    }

    /// Locking clears a copied secret right away.
    pub fn clear_now(&self) {
        let generation = self.last.lock().map(|(g, _)| g);
        if let Some(generation) = generation {
            self.clear_if_ours(generation);
        }
    }
}
