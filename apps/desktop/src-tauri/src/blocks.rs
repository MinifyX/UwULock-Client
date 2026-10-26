//! macOS: completion blocks for system calls that answer over XPC (Safari,
//! AutoFill). NSXPC needs the block's type signature to send the answer back
//! and raises an Objective-C exception without one, which aborts the app —
//! `RcBlock::new` makes blocks without. These carry it, and [`guarded`] turns
//! any exception that still comes into `None`.
//!
//! The strings are what block2 computes for the closure's Rust types (checked
//! in debug builds): `*mut AnyObject` is `@`, `Bool` is `B` on Apple silicon
//! and `c` on Intel.

use block2::ManualBlockEncoding;
use objc2::runtime::{AnyObject, Bool};
use std::ffi::CStr;
use std::panic::AssertUnwindSafe;

/// `void (^)(id)`: an object or an error.
pub(crate) struct Object;
// SAFETY: one object argument, no return value; 64-bit offsets.
unsafe impl ManualBlockEncoding for Object {
    type Arguments = (*mut AnyObject,);
    type Return = ();
    const ENCODING_CSTR: &'static CStr = c"v16@?0@8";
}

/// `void (^)(id, NSError *)`.
pub(crate) struct ObjectError;
// SAFETY: two object arguments, no return value; 64-bit offsets.
unsafe impl ManualBlockEncoding for ObjectError {
    type Arguments = (*mut AnyObject, *mut AnyObject);
    type Return = ();
    const ENCODING_CSTR: &'static CStr = c"v24@?0@8@16";
}

/// `void (^)(BOOL)`.
pub(crate) struct Flag;
// SAFETY: one BOOL argument, no return value; 64-bit offsets, BOOL unpadded as block2 counts.
unsafe impl ManualBlockEncoding for Flag {
    type Arguments = (Bool,);
    type Return = ();
    #[cfg(target_arch = "aarch64")]
    const ENCODING_CSTR: &'static CStr = c"v9@?0B8";
    #[cfg(not(target_arch = "aarch64"))]
    const ENCODING_CSTR: &'static CStr = c"v9@?0c8";
}

/// `void (^)(BOOL, NSError *)`.
pub(crate) struct FlagError;
// SAFETY: a BOOL and an object argument, no return value; offsets as block2 counts them.
unsafe impl ManualBlockEncoding for FlagError {
    type Arguments = (Bool, *mut AnyObject);
    type Return = ();
    #[cfg(target_arch = "aarch64")]
    const ENCODING_CSTR: &'static CStr = c"v17@?0B8@9";
    #[cfg(not(target_arch = "aarch64"))]
    const ENCODING_CSTR: &'static CStr = c"v17@?0c8@9";
}

/// Runs `call`; an Objective-C exception it raises becomes `None` (and a line
/// in the log) instead of aborting the app.
pub(crate) fn guarded<R>(what: &str, call: impl FnOnce() -> R) -> Option<R> {
    match objc2::exception::catch(AssertUnwindSafe(call)) {
        Ok(value) => Some(value),
        Err(exception) => {
            tracing::warn!(?exception, "{what}: the system raised an exception");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use block2::RcBlock;

    /// block2 compares the strings with the closures' types in debug builds:
    /// making each block is the check.
    #[test]
    fn encodings_match_the_closures() {
        let _ = RcBlock::with_encoding::<_, _, _, Object>(|_: *mut AnyObject| {});
        let _ = RcBlock::with_encoding::<_, _, _, ObjectError>(
            |_: *mut AnyObject, _: *mut AnyObject| {},
        );
        let _ = RcBlock::with_encoding::<_, _, _, Flag>(|_: Bool| {});
        let _ = RcBlock::with_encoding::<_, _, _, FlagError>(|_: Bool, _: *mut AnyObject| {});
    }

    #[test]
    fn an_exception_becomes_none() {
        let object = objc2::runtime::NSObject::new();
        let raised = guarded("test", || {
            // SAFETY: `performSelector:` exists; the selector it is given doesn't, so NSObject
            // raises NSInvalidArgumentException, as an XPC call without a block signature does.
            let _: *mut AnyObject =
                unsafe { objc2::msg_send![&*object, performSelector: objc2::sel!(uwulockNothing)] };
        });
        assert!(raised.is_none());
        assert_eq!(guarded("test", || 7), Some(7));
    }
}
