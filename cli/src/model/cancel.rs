//! The cancel of `grammachy model download`, spec section 5.3.
//!
//! The shell has no way to stop a transfer other than to signal the process it
//! started, so a SIGTERM is the cancel. What the user must not lose is the
//! `.part` file: a resumed download is minutes rather than an hour, and the
//! whole point of Cancel is that it is cheap to change one's mind.
//!
//! So the default disposition will not do. It would end this process while curl
//! carried on as an orphan, still writing the file nobody is waiting for. The
//! handler here does the one thing a signal handler may safely do, which is to
//! set a flag; [`crate::model::curl`] polls that flag, kills its child, and
//! answers `Transfer::Cancelled`.

use std::sync::atomic::{AtomicBool, Ordering};

/// Whether a cancel has arrived. Set from the handler, read from the transfer.
static CANCELLED: AtomicBool = AtomicBool::new(false);

/// Take SIGTERM and SIGINT over for the rest of this run.
///
/// Only a download calls this: every other verb finishes in milliseconds and
/// has nothing a signal could spoil.
pub fn listen() {
    // Safety: `signal` installs the handler and returns the old one, which
    // nothing here needs. `on_signal` only stores into an atomic, so it is
    // async-signal-safe.
    unsafe {
        libc::signal(libc::SIGTERM, on_signal as *const () as libc::sighandler_t);
        libc::signal(libc::SIGINT, on_signal as *const () as libc::sighandler_t);
    }
}

extern "C" fn on_signal(_signal: libc::c_int) {
    CANCELLED.store(true, Ordering::SeqCst);
}

/// Whether the run has been asked to stop.
pub fn requested() -> bool {
    CANCELLED.load(Ordering::SeqCst)
}

/// Ask the run to stop, without a signal. This is the seam a test uses.
pub fn request() {
    CANCELLED.store(true, Ordering::SeqCst);
}

/// Forget an earlier cancel. This is the seam a test uses.
pub fn reset() {
    CANCELLED.store(false, Ordering::SeqCst);
}
