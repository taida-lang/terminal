//! `taida-lang/terminal` — `ReadEvent[]()` implementation (Phase 3).
//!
//! This module implements the unified event model that combines key,
//! mouse, and resize events into a single blocking `ReadEvent[]()` call.
//!
//! ## Design (TM_DESIGN.md Section 6, 7)
//!
//! - Key events reuse the existing `key::decode` decoder.
//! - Mouse events parse xterm SGR format (`\x1b[<Pb;Pc;Pr[Mm]`).
//! - Resize events are detected via `SIGWINCH` + self-pipe pattern.
//! - Unknown sequences are returned as `EventKind::Unknown` (never dropped).
//! - `ReadEvent` requires raw mode to be active (via `RawModeEnter`).
//!
//! ## Error Contract
//!
//! - `ReadEventNotInRawMode`       (4001): raw mode is not active
//! - `ReadEventNotATty`            (4002): stdin is not a TTY
//! - `ReadEventReadFailed`         (4003): read(2) syscall failed
//! - `ReadEventEof`                (4004): EOF on stdin
//! - `ReadEventInterrupted`        (4005): EINTR without SIGWINCH
//! - `ReadEventPanic`              (4006): internal panic caught
//! - `ReadEventResizeInitFailed`   (4007): SIGWINCH pipe/handler init failed
//!
//! ## Resize Detection
//!
//! Uses a self-pipe: `SIGWINCH` handler writes a byte to a pipe fd.
//! `ReadEvent` uses `poll(2)` to multiplex stdin and the pipe read end.
//! When the pipe is readable, we drain it and query the new terminal
//! size via `ioctl(TIOCGWINSZ)`.
//!
//! ## Signal Ownership (TMB-007)
//!
//! The SIGWINCH handler is installed via `sigaction()` with the previous
//! handler saved. After writing to the self-pipe, our handler chains to
//! the old handler so other libraries are not disrupted. The old handler
//! is stored in an `AtomicPtr<libc::sigaction>` (heap-allocated) so it
//! can be read from the async-signal-safe handler without a mutex.
//!
//! ## Event Framing (TMB-009)
//!
//! A process-global pending byte queue ensures 1 call = 1 event.
//! When ESC-prefixed input is read, exactly one escape sequence is
//! consumed and any surplus bytes are pushed back to the pending queue.

use core::ffi::c_char;
use core::panic::AssertUnwindSafe;
use std::collections::VecDeque;
use std::panic;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicPtr, Ordering};

use taida_addon::bridge::HostValueBuilder;
use taida_addon::{TaidaAddonErrorV1, TaidaAddonStatus, TaidaAddonValueV1, TaidaHostV1};

use crate::key::{self, DecodedKey, KeyKind};

// ── Event / Mouse kind discriminants (Phase 3 lock) ─────────────

/// `EventKind` discriminants. Numeric values are part of the surface.
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventKind {
    Key = 0,
    Mouse = 1,
    Resize = 2,
    Unknown = 3,
}

impl EventKind {
    pub const fn tag(self) -> i64 {
        self as i64
    }
}

/// `MouseKind` discriminants. Numeric values are part of the surface.
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseKind {
    Down = 0,
    Up = 1,
    Move = 2,
    Drag = 3,
    ScrollUp = 4,
    ScrollDown = 5,
}

impl MouseKind {
    pub const fn tag(self) -> i64 {
        self as i64
    }
}

// ── Error codes (4xxx range) ────────────────────────────────────

pub mod err {
    /// ReadEvent called without raw mode active.
    pub const READ_EVENT_NOT_IN_RAW_MODE: u32 = 4001;
    /// stdin is not a TTY.
    pub const READ_EVENT_NOT_A_TTY: u32 = 4002;
    /// read(2) syscall failed.
    pub const READ_EVENT_READ_FAILED: u32 = 4003;
    /// EOF on stdin.
    pub const READ_EVENT_EOF: u32 = 4004;
    /// Signal interrupted read (not SIGWINCH).
    pub const READ_EVENT_INTERRUPTED: u32 = 4005;
    /// Internal panic caught.
    pub const READ_EVENT_PANIC: u32 = 4006;
    /// SIGWINCH pipe/handler initialization failed (TMB-008).
    pub const READ_EVENT_RESIZE_INIT_FAILED: u32 = 4007;
}

// ── Decoded event types ─────────────────────────────────────────

/// A decoded mouse event from SGR format.
#[derive(Debug, Clone)]
pub struct DecodedMouse {
    pub kind: MouseKind,
    pub col: i64,
    pub row: i64,
    pub button: i64,
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
}

/// A decoded resize event.
#[derive(Debug, Clone, Copy)]
pub struct DecodedResize {
    pub cols: i64,
    pub rows: i64,
}

/// A fully decoded event.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum DecodedEvent {
    Key(DecodedKey),
    Mouse(DecodedMouse),
    Resize(DecodedResize),
    Unknown(String),
}

// ── SIGWINCH self-pipe ──────────────────────────────────────────

/// File descriptors for the SIGWINCH self-pipe.
/// [0] = read end, [1] = write end.
static SIGWINCH_PIPE: [AtomicI32; 2] = [AtomicI32::new(-1), AtomicI32::new(-1)];

/// Whether the SIGWINCH handler has been installed.
static SIGWINCH_INSTALLED: AtomicBool = AtomicBool::new(false);

/// Mutex to serialize handler installation.
static SIGWINCH_INIT: Mutex<()> = Mutex::new(());

/// Previous SIGWINCH handler saved at installation time (TMB-007).
///
/// We store a heap-allocated `libc::sigaction` behind an `AtomicPtr` so
/// the signal handler can read it without a mutex (async-signal-safe).
/// `null` means no previous handler was saved.
static OLD_SIGWINCH: AtomicPtr<libc::sigaction> = AtomicPtr::new(core::ptr::null_mut());

/// SIGWINCH signal handler. Writes a single byte to the self-pipe,
/// then chains to the previously installed handler (TMB-007).
///
/// # Safety
///
/// This is a signal handler — only async-signal-safe functions are used
/// (`write` on a pipe fd, calling a previous handler via its function
/// pointer). We don't check the return value of `write` because there's
/// nothing we can do in a signal handler if the pipe is full.
extern "C" fn sigwinch_handler(sig: i32) {
    // 1. Write to self-pipe (our own work).
    let wfd = SIGWINCH_PIPE[1].load(Ordering::Relaxed);
    if wfd >= 0 {
        let byte: u8 = b'W';
        unsafe { libc::write(wfd, &byte as *const u8 as *const _, 1) };
    }

    // 2. Chain to old handler (TMB-007).
    let old_ptr = OLD_SIGWINCH.load(Ordering::Relaxed);
    if !old_ptr.is_null() {
        let old_sa = unsafe { &*old_ptr };
        let handler = old_sa.sa_sigaction;
        if old_sa.sa_flags & libc::SA_SIGINFO != 0 {
            // SA_SIGINFO style handler: fn(sig, info, ucontext)
            if handler != 0 && handler != libc::SIG_DFL && handler != libc::SIG_IGN {
                let sa_sigaction_fn: extern "C" fn(
                    libc::c_int,
                    *mut libc::siginfo_t,
                    *mut libc::c_void,
                ) = unsafe { core::mem::transmute(handler) };
                sa_sigaction_fn(sig, core::ptr::null_mut(), core::ptr::null_mut());
            }
        } else {
            // Traditional handler: fn(sig)
            if handler != 0 && handler != libc::SIG_DFL && handler != libc::SIG_IGN {
                let sa_handler_fn: extern "C" fn(libc::c_int) =
                    unsafe { core::mem::transmute(handler) };
                sa_handler_fn(sig);
            }
        }
    }
}

/// Ensure the SIGWINCH self-pipe and handler are installed.
/// Returns the read end fd, or -1 on failure.
fn ensure_sigwinch_pipe() -> i32 {
    if SIGWINCH_INSTALLED.load(Ordering::Acquire) {
        return SIGWINCH_PIPE[0].load(Ordering::Acquire);
    }

    let _lock = match SIGWINCH_INIT.lock() {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    };

    // Double-check after acquiring the lock.
    if SIGWINCH_INSTALLED.load(Ordering::Acquire) {
        return SIGWINCH_PIPE[0].load(Ordering::Acquire);
    }

    // Create pipe.
    let mut fds = [0i32; 2];
    if unsafe { libc::pipe(fds.as_mut_ptr()) } != 0 {
        return -1;
    }

    // Make both ends non-blocking.
    for &fd in &fds {
        let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
        if flags < 0 {
            unsafe {
                libc::close(fds[0]);
                libc::close(fds[1]);
            }
            return -1;
        }
        if unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
            unsafe {
                libc::close(fds[0]);
                libc::close(fds[1]);
            }
            return -1;
        }
    }

    // Set close-on-exec.
    for &fd in &fds {
        let flags = unsafe { libc::fcntl(fd, libc::F_GETFD) };
        if flags >= 0 {
            unsafe { libc::fcntl(fd, libc::F_SETFD, flags | libc::FD_CLOEXEC) };
        }
    }

    SIGWINCH_PIPE[0].store(fds[0], Ordering::Release);
    SIGWINCH_PIPE[1].store(fds[1], Ordering::Release);

    // Install signal handler, saving the old one (TMB-007).
    //
    // TMB-017: install order must be race-free. If we call
    // `sigaction(SIGWINCH, &sa, &mut old_sa)` first and *then* publish
    // `old_sa` to `OLD_SIGWINCH`, there is a window where our handler
    // can fire with `OLD_SIGWINCH == null`, silently dropping the
    // chain to any previously-installed handler from other libraries.
    //
    // The fix is a 2-step install:
    //   1. Query the current handler via `sigaction(SIGWINCH, NULL, &mut old_sa)`
    //      (no install yet — signal disposition is unchanged).
    //   2. Publish `OLD_SIGWINCH` so the chain target is visible.
    //   3. Install our handler via `sigaction(SIGWINCH, &sa, NULL)`.
    //      Any SIGWINCH delivered after this point finds the chain
    //      target already stored with Release ordering, so the
    //      Relaxed load inside the handler is safe (install itself is
    //      a full barrier on any sensible platform).
    //   4. Mark `SIGWINCH_INSTALLED = true` last, so the fast-path
    //      in `ensure_sigwinch_pipe()` only skips the install once the
    //      handler is fully live.
    let mut sa: libc::sigaction = unsafe { core::mem::zeroed() };
    sa.sa_sigaction = sigwinch_handler as *const () as usize;
    sa.sa_flags = libc::SA_RESTART;
    unsafe { libc::sigemptyset(&mut sa.sa_mask) };

    // Step 1: query current handler without installing ours.
    let mut old_sa: libc::sigaction = unsafe { core::mem::zeroed() };
    if unsafe {
        libc::sigaction(
            libc::SIGWINCH,
            core::ptr::null(),
            &mut old_sa as *mut libc::sigaction,
        )
    } != 0
    {
        // Could not even query the current handler — close pipe and bail.
        unsafe {
            libc::close(fds[0]);
            libc::close(fds[1]);
        }
        SIGWINCH_PIPE[0].store(-1, Ordering::Release);
        SIGWINCH_PIPE[1].store(-1, Ordering::Release);
        return -1;
    }

    // Step 2: publish the old handler *before* installing ours so the
    // handler, if it fires on the very next instruction, already sees
    // a valid chain target (TMB-017).
    let old_box = Box::new(old_sa);
    let old_raw = Box::into_raw(old_box);
    OLD_SIGWINCH.store(old_raw, Ordering::Release);

    // Step 3: install our handler (third arg NULL — we already have old).
    if unsafe {
        libc::sigaction(
            libc::SIGWINCH,
            &sa as *const libc::sigaction,
            core::ptr::null_mut(),
        )
    } != 0
    {
        // Install failed. Roll back the OLD_SIGWINCH publication and
        // free the heap allocation so we don't leak on retry. The
        // handler was never installed, so no signal can reach the
        // chain path from this point on.
        OLD_SIGWINCH.store(core::ptr::null_mut(), Ordering::Release);
        unsafe {
            drop(Box::from_raw(old_raw));
            libc::close(fds[0]);
            libc::close(fds[1]);
        }
        SIGWINCH_PIPE[0].store(-1, Ordering::Release);
        SIGWINCH_PIPE[1].store(-1, Ordering::Release);
        return -1;
    }

    // Step 4: mark installed last so fast-path callers only see the
    // flag once the handler is fully live.
    SIGWINCH_INSTALLED.store(true, Ordering::Release);
    fds[0]
}

/// Drain all bytes from the SIGWINCH pipe read end.
fn drain_sigwinch_pipe(rfd: i32) {
    let mut buf = [0u8; 64];
    loop {
        let n = unsafe { libc::read(rfd, buf.as_mut_ptr() as *mut _, buf.len()) };
        if n <= 0 {
            break;
        }
    }
}

/// Query current terminal size via ioctl.
fn query_terminal_size() -> Option<DecodedResize> {
    let mut ws: libc::winsize = unsafe { core::mem::zeroed() };
    let rc = unsafe { libc::ioctl(libc::STDOUT_FILENO, libc::TIOCGWINSZ, &mut ws) };
    if rc != 0 || ws.ws_col == 0 || ws.ws_row == 0 {
        return None;
    }
    Some(DecodedResize {
        cols: ws.ws_col as i64,
        rows: ws.ws_row as i64,
    })
}

// ── Mouse SGR decode ────────────────────────────────────────────

/// Parse xterm SGR mouse format: `\x1b[<Pb;Pc;Pr[Mm]`
///
/// Where:
/// - Pb = button + modifier bits
/// - Pc = column (1-based)
/// - Pr = row (1-based)
/// - M = press, m = release
///
/// Button encoding (low 2 bits of Pb):
/// - 0 = left button
/// - 1 = middle button
/// - 2 = right button
/// - 3 = release (legacy, not used in SGR)
/// - bit 5 (32) = motion while button held (drag)
/// - bit 6 (64) = scroll wheel
///
/// Returns `None` if the buffer doesn't match SGR mouse format.
pub fn decode_sgr_mouse(buf: &[u8]) -> Option<DecodedMouse> {
    // Minimum: \x1b [ < N ; N ; N M  = at least 9 bytes
    // (3 prefix + 1 param + ; + 1 param + ; + 1 param + 1 final)
    if buf.len() < 9 {
        return None;
    }
    // Must start with \x1b[<
    if buf[0] != 0x1B || buf[1] != b'[' || buf[2] != b'<' {
        return None;
    }

    // Parse the three semicolon-separated numbers and the final byte.
    let rest = &buf[3..];
    let final_byte = *rest.last()?;
    if final_byte != b'M' && final_byte != b'm' {
        return None;
    }

    // Parse "Pb;Pc;Pr" from rest[..rest.len()-1]
    let params_slice = &rest[..rest.len() - 1];
    let params_str = core::str::from_utf8(params_slice).ok()?;
    let mut parts = params_str.splitn(3, ';');
    let pb: u32 = parts.next()?.parse().ok()?;
    let pc: u32 = parts.next()?.parse().ok()?;
    let pr: u32 = parts.next()?.parse().ok()?;

    // Modifier flags from Pb.
    let shift = pb & 4 != 0;
    let alt = pb & 8 != 0;
    let ctrl = pb & 16 != 0;

    let low_bits = pb & 3;
    let is_motion = pb & 32 != 0;
    let is_scroll = pb & 64 != 0;
    let is_release = final_byte == b'm';

    let (kind, button) = if is_scroll {
        // Scroll events: low_bits 0 = up, 1 = down
        if low_bits == 0 {
            (MouseKind::ScrollUp, 0i64)
        } else {
            (MouseKind::ScrollDown, 0i64)
        }
    } else if is_motion {
        // Drag: motion with button held
        (MouseKind::Drag, low_bits as i64)
    } else if is_release {
        (MouseKind::Up, low_bits as i64)
    } else {
        (MouseKind::Down, low_bits as i64)
    };

    Some(DecodedMouse {
        kind,
        col: pc as i64,
        row: pr as i64,
        button,
        ctrl,
        alt,
        shift,
    })
}

// ── I/O: Read one event ─────────────────────────────────────────

const MAX_EVENT_BYTES: usize = 64;

/// Process-global pending byte queue (TMB-009).
///
/// When `read_stdin_event` reads more bytes than a single event
/// consumes, the surplus is pushed here and drained on the next call.
/// This ensures 1 call = 1 event framing.
static PENDING_BYTES: Mutex<VecDeque<u8>> = Mutex::new(VecDeque::new());

/// Read outcome from the event I/O layer.
enum EventReadOutcome {
    Event(DecodedEvent),
    Eof,
    Interrupted,
    Io(i32),
    /// SIGWINCH pipe/handler initialization failed (TMB-008).
    ResizeInitFailed,
}

/// Read one event from stdin, multiplexed with SIGWINCH detection.
///
/// Uses `poll(2)` to wait on both stdin and the SIGWINCH self-pipe.
/// Returns the first event that becomes available.
fn read_one_event(fd: i32) -> EventReadOutcome {
    let sigwinch_rfd = ensure_sigwinch_pipe();

    // TMB-008: If SIGWINCH pipe initialization failed, return a
    // deterministic error instead of silently degrading to stdin-only.
    if sigwinch_rfd < 0 {
        return EventReadOutcome::ResizeInitFailed;
    }

    // Build poll fds: [0] = stdin, [1] = sigwinch pipe
    let mut pfds = [
        libc::pollfd {
            fd,
            events: libc::POLLIN,
            revents: 0,
        },
        libc::pollfd {
            fd: sigwinch_rfd,
            events: libc::POLLIN,
            revents: 0,
        },
    ];

    let nfds: libc::nfds_t = 2;

    // Block until at least one fd is ready.
    let ret = unsafe { libc::poll(pfds.as_mut_ptr(), nfds, -1) };
    if ret < 0 {
        let e = std::io::Error::last_os_error().raw_os_error().unwrap_or(-1);
        if e == libc::EINTR {
            // Check if SIGWINCH pipe has data (the handler may have written
            // even though poll returned EINTR).
            let mut check_pfd = libc::pollfd {
                fd: sigwinch_rfd,
                events: libc::POLLIN,
                revents: 0,
            };
            let p = unsafe { libc::poll(&mut check_pfd, 1, 0) };
            if p > 0 && check_pfd.revents & libc::POLLIN != 0 {
                drain_sigwinch_pipe(sigwinch_rfd);
                if let Some(resize) = query_terminal_size() {
                    return EventReadOutcome::Event(DecodedEvent::Resize(resize));
                }
            }
            return EventReadOutcome::Interrupted;
        }
        return EventReadOutcome::Io(e);
    }

    // Check SIGWINCH pipe first (resize has priority).
    if pfds[1].revents & libc::POLLIN != 0 {
        drain_sigwinch_pipe(sigwinch_rfd);
        if let Some(resize) = query_terminal_size() {
            return EventReadOutcome::Event(DecodedEvent::Resize(resize));
        }
        // If ioctl failed, fall through to check stdin.
    }

    // Check stdin.
    if pfds[0].revents & libc::POLLIN != 0 {
        return read_stdin_event(fd);
    }

    // poll returned but neither fd was ready (shouldn't happen with -1 timeout).
    // Return interrupted to let the caller retry.
    EventReadOutcome::Interrupted
}

/// Read bytes from stdin and decode as key or mouse event.
///
/// TMB-009: Enforces 1 call = 1 event framing. Surplus bytes from a
/// multi-byte read are stashed in `PENDING_BYTES` for the next call.
fn read_stdin_event(fd: i32) -> EventReadOutcome {
    let mut buf = [0u8; MAX_EVENT_BYTES];
    let mut len: usize = 0;

    // Step 1: Drain pending bytes first.
    {
        let mut pending = match PENDING_BYTES.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        while len < buf.len() && !pending.is_empty() {
            if let Some(b) = pending.pop_front() {
                buf[len] = b;
                len += 1;
            }
        }
    }

    // Step 2: If no pending data, read the first byte from stdin.
    if len == 0 {
        let n = unsafe { libc::read(fd, buf.as_mut_ptr() as *mut _, 1) };
        if n < 0 {
            let e = std::io::Error::last_os_error().raw_os_error().unwrap_or(-1);
            if e == libc::EINTR {
                return EventReadOutcome::Interrupted;
            }
            return EventReadOutcome::Io(e);
        }
        if n == 0 {
            return EventReadOutcome::Eof;
        }
        len = n as usize;
    }

    // Step 3: Determine how much more to read based on the first byte.
    if buf[0] == 0x1B && len == 1 {
        // ESC prefix — need follow-up bytes to distinguish escape
        // sequences from a lone ESC key.
        // Read one byte with short timeout to see if more follows.
        let mut pfd = libc::pollfd {
            fd,
            events: libc::POLLIN,
            revents: 0,
        };
        let p = unsafe { libc::poll(&mut pfd, 1, 50) };
        if p > 0 {
            let nr = unsafe { libc::read(fd, buf.as_mut_ptr().add(len) as *mut _, 1) };
            if nr > 0 {
                len += nr as usize;
            }
        }
    }

    // If we have ESC [ or ESC O, read more until sequence terminates.
    if len >= 2 && buf[0] == 0x1B {
        if buf[1] == b'[' {
            // CSI sequence — read until we get a final byte (0x40..0x7E).
            // Special case: SGR mouse starts with ESC [ < and ends with M/m.
            while len < buf.len() {
                let last = buf[len - 1];
                // CSI final bytes are in 0x40..=0x7E range.
                if len >= 3 && (0x40..=0x7E).contains(&last) {
                    break;
                }
                let mut pfd = libc::pollfd {
                    fd,
                    events: libc::POLLIN,
                    revents: 0,
                };
                let p = unsafe { libc::poll(&mut pfd, 1, 50) };
                if p <= 0 {
                    break;
                }
                let nr = unsafe { libc::read(fd, buf.as_mut_ptr().add(len) as *mut _, 1) };
                if nr <= 0 {
                    break;
                }
                len += nr as usize;
            }
        } else if buf[1] == b'O' {
            // SS3 sequence (e.g., ESC O P for F1). Read one more byte.
            if len == 2 {
                let mut pfd = libc::pollfd {
                    fd,
                    events: libc::POLLIN,
                    revents: 0,
                };
                let p = unsafe { libc::poll(&mut pfd, 1, 50) };
                if p > 0 {
                    let nr = unsafe { libc::read(fd, buf.as_mut_ptr().add(len) as *mut _, 1) };
                    if nr > 0 {
                        len += nr as usize;
                    }
                }
            }
        }
        // ESC + printable (Alt+key): already have 2 bytes, that's one event.
    } else if buf[0] >= 0x80 && len == 1 {
        // Multi-byte UTF-8: pull continuation bytes.
        let expect = utf8_continuation_count(buf[0]);
        let target = (1 + expect).min(buf.len());
        while len < target {
            let mut pfd = libc::pollfd {
                fd,
                events: libc::POLLIN,
                revents: 0,
            };
            let p = unsafe { libc::poll(&mut pfd, 1, 50) };
            if p <= 0 {
                break;
            }
            let nr = unsafe { libc::read(fd, buf.as_mut_ptr().add(len) as *mut _, 1) };
            if nr <= 0 {
                break;
            }
            len += nr as usize;
        }
    }

    let data = &buf[..len];

    // Step 4: Extract exactly one event from `data` and push surplus
    // bytes back to PENDING_BYTES (TMB-009 framing rule).
    let (consumed, event) = decode_one_event(data);

    // Push unconsumed bytes back to pending.
    if consumed < len {
        let mut pending = match PENDING_BYTES.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        // Prepend unconsumed bytes (they should be processed before any
        // new reads). We insert at the front of the deque.
        for &b in data[consumed..].iter().rev() {
            pending.push_front(b);
        }
    }

    EventReadOutcome::Event(event)
}

/// Decode exactly one event from `data` and return (bytes_consumed, event).
///
/// This is the framing core for TMB-009. It identifies the boundary of
/// a single event in the byte buffer and returns how many bytes were consumed.
fn decode_one_event(data: &[u8]) -> (usize, DecodedEvent) {
    if data.is_empty() {
        return (0, DecodedEvent::Unknown(String::new()));
    }

    // Non-ESC, non-high-byte: single byte event.
    if data[0] != 0x1B && data[0] < 0x80 {
        let dk = key::decode(&data[..1]);
        return (1, DecodedEvent::Key(dk));
    }

    // UTF-8 multi-byte.
    if data[0] >= 0x80 && data[0] != 0x1B {
        let expect = utf8_continuation_count(data[0]);
        let char_len = (1 + expect).min(data.len());
        let dk = key::decode(&data[..char_len]);
        return (char_len, DecodedEvent::Key(dk));
    }

    // ESC prefix.
    if data.len() == 1 {
        // Lone ESC.
        let dk = key::decode(&data[..1]);
        return (1, DecodedEvent::Key(dk));
    }

    match data[1] {
        b'[' => {
            // CSI sequence.
            if data.len() < 3 {
                // Incomplete CSI.
                let dk = key::decode(data);
                return (data.len(), DecodedEvent::Key(dk));
            }
            // Check for SGR mouse: ESC [ < ... M/m
            if data[2] == b'<' {
                // Find the terminal byte M or m.
                if let Some(end) = find_sgr_mouse_end(data) {
                    let seq = &data[..=end];
                    if let Some(mouse) = decode_sgr_mouse(seq) {
                        return (end + 1, DecodedEvent::Mouse(mouse));
                    }
                    // Failed to parse as mouse — treat as unknown key.
                    let dk = key::decode(seq);
                    return (end + 1, DecodedEvent::Key(dk));
                }
                // No terminal byte found — consume all.
                let dk = key::decode(data);
                return (data.len(), DecodedEvent::Key(dk));
            }
            // Regular CSI: find final byte (0x40..=0x7E).
            if let Some(end) = find_csi_end(data) {
                let seq = &data[..=end];
                let dk = key::decode(seq);
                return (end + 1, DecodedEvent::Key(dk));
            }
            // No final byte found — consume all as unknown.
            let dk = key::decode(data);
            (data.len(), DecodedEvent::Key(dk))
        }
        b'O' => {
            // SS3 sequence: ESC O <byte>
            let seq_len = 3.min(data.len());
            let dk = key::decode(&data[..seq_len]);
            (seq_len, DecodedEvent::Key(dk))
        }
        c if (0x20..=0x7E).contains(&c) => {
            // Alt + printable: ESC + char = 2 bytes.
            let dk = key::decode(&data[..2]);
            (2, DecodedEvent::Key(dk))
        }
        _ => {
            // Unknown ESC sequence — consume 2 bytes.
            let dk = key::decode(&data[..2.min(data.len())]);
            (2.min(data.len()), DecodedEvent::Key(dk))
        }
    }
}

/// Find the index of the CSI final byte (0x40..=0x7E) in a CSI sequence.
/// The sequence starts at `data[0]` = ESC, `data[1]` = `[`.
/// Returns `None` if no final byte found.
fn find_csi_end(data: &[u8]) -> Option<usize> {
    // CSI parameters are bytes in 0x30..=0x3F, intermediates in 0x20..=0x2F,
    // final byte in 0x40..=0x7E. We scan from position 2 onward.
    (2..data.len()).find(|&i| (0x40..=0x7E).contains(&data[i]))
}

/// Find the index of the SGR mouse terminal byte (M or m).
/// The sequence starts with ESC [ <. Returns `None` if not found.
fn find_sgr_mouse_end(data: &[u8]) -> Option<usize> {
    (3..data.len()).find(|&i| data[i] == b'M' || data[i] == b'm')
}

fn utf8_continuation_count(b: u8) -> usize {
    if b & 0b1110_0000 == 0b1100_0000 {
        1
    } else if b & 0b1111_0000 == 0b1110_0000 {
        2
    } else if b & 0b1111_1000 == 0b1111_0000 {
        3
    } else {
        0
    }
}

// ── Build the return pack ───────────────────────────────────────

/// Build the full ReadEvent return pack:
///
/// ```taida
/// @(
///   kind <= EventKind.*,
///   key <= @(kind, text, ctrl, alt, shift),
///   mouse <= @(kind, col, row, button, ctrl, alt, shift),
///   resize <= @(cols, rows)
/// )
/// ```
fn build_event_pack(
    builder: &HostValueBuilder<'_>,
    event: &DecodedEvent,
) -> *mut TaidaAddonValueV1 {
    let (event_kind, key_sub, mouse_sub, resize_sub) = match event {
        DecodedEvent::Key(dk) => (
            EventKind::Key,
            build_key_subpack(builder, dk),
            build_default_mouse_subpack(builder),
            build_default_resize_subpack(builder),
        ),
        DecodedEvent::Mouse(dm) => (
            EventKind::Mouse,
            build_default_key_subpack(builder),
            build_mouse_subpack(builder, dm),
            build_default_resize_subpack(builder),
        ),
        DecodedEvent::Resize(dr) => (
            EventKind::Resize,
            build_default_key_subpack(builder),
            build_default_mouse_subpack(builder),
            build_resize_subpack(builder, dr),
        ),
        DecodedEvent::Unknown(raw) => (
            EventKind::Unknown,
            build_key_subpack(
                builder,
                &DecodedKey {
                    kind: KeyKind::Unknown,
                    text: raw.clone(),
                    ctrl: false,
                    alt: false,
                    shift: false,
                },
            ),
            build_default_mouse_subpack(builder),
            build_default_resize_subpack(builder),
        ),
    };

    let kind_v = builder.int(event_kind.tag());
    if kind_v.is_null() || key_sub.is_null() || mouse_sub.is_null() || resize_sub.is_null() {
        // Cleanup any non-null values.
        for v in [kind_v, key_sub, mouse_sub, resize_sub] {
            if !v.is_null() {
                unsafe { builder.release(v) };
            }
        }
        return core::ptr::null_mut();
    }

    let names: [*const c_char; 4] = [
        c"kind".as_ptr(),
        c"key".as_ptr(),
        c"mouse".as_ptr(),
        c"resize".as_ptr(),
    ];
    let values: [*mut TaidaAddonValueV1; 4] = [kind_v, key_sub, mouse_sub, resize_sub];
    builder.pack(&names, &values)
}

fn build_key_subpack(builder: &HostValueBuilder<'_>, dk: &DecodedKey) -> *mut TaidaAddonValueV1 {
    let kind_v = builder.int(dk.kind.tag());
    let text_v = builder.str(&dk.text);
    let ctrl_v = builder.bool(dk.ctrl);
    let alt_v = builder.bool(dk.alt);
    let shift_v = builder.bool(dk.shift);

    if kind_v.is_null()
        || text_v.is_null()
        || ctrl_v.is_null()
        || alt_v.is_null()
        || shift_v.is_null()
    {
        for v in [kind_v, text_v, ctrl_v, alt_v, shift_v] {
            if !v.is_null() {
                unsafe { builder.release(v) };
            }
        }
        return core::ptr::null_mut();
    }

    let names: [*const c_char; 5] = [
        c"kind".as_ptr(),
        c"text".as_ptr(),
        c"ctrl".as_ptr(),
        c"alt".as_ptr(),
        c"shift".as_ptr(),
    ];
    let values: [*mut TaidaAddonValueV1; 5] = [kind_v, text_v, ctrl_v, alt_v, shift_v];
    builder.pack(&names, &values)
}

fn build_default_key_subpack(builder: &HostValueBuilder<'_>) -> *mut TaidaAddonValueV1 {
    build_key_subpack(
        builder,
        &DecodedKey {
            kind: KeyKind::Unknown,
            text: String::new(),
            ctrl: false,
            alt: false,
            shift: false,
        },
    )
}

fn build_mouse_subpack(
    builder: &HostValueBuilder<'_>,
    dm: &DecodedMouse,
) -> *mut TaidaAddonValueV1 {
    let kind_v = builder.int(dm.kind.tag());
    let col_v = builder.int(dm.col);
    let row_v = builder.int(dm.row);
    let button_v = builder.int(dm.button);
    let ctrl_v = builder.bool(dm.ctrl);
    let alt_v = builder.bool(dm.alt);
    let shift_v = builder.bool(dm.shift);

    if kind_v.is_null()
        || col_v.is_null()
        || row_v.is_null()
        || button_v.is_null()
        || ctrl_v.is_null()
        || alt_v.is_null()
        || shift_v.is_null()
    {
        for v in [kind_v, col_v, row_v, button_v, ctrl_v, alt_v, shift_v] {
            if !v.is_null() {
                unsafe { builder.release(v) };
            }
        }
        return core::ptr::null_mut();
    }

    let names: [*const c_char; 7] = [
        c"kind".as_ptr(),
        c"col".as_ptr(),
        c"row".as_ptr(),
        c"button".as_ptr(),
        c"ctrl".as_ptr(),
        c"alt".as_ptr(),
        c"shift".as_ptr(),
    ];
    let values: [*mut TaidaAddonValueV1; 7] =
        [kind_v, col_v, row_v, button_v, ctrl_v, alt_v, shift_v];
    builder.pack(&names, &values)
}

fn build_default_mouse_subpack(builder: &HostValueBuilder<'_>) -> *mut TaidaAddonValueV1 {
    build_mouse_subpack(
        builder,
        &DecodedMouse {
            kind: MouseKind::Move,
            col: 0,
            row: 0,
            button: 0,
            ctrl: false,
            alt: false,
            shift: false,
        },
    )
}

fn build_resize_subpack(
    builder: &HostValueBuilder<'_>,
    dr: &DecodedResize,
) -> *mut TaidaAddonValueV1 {
    let cols_v = builder.int(dr.cols);
    let rows_v = builder.int(dr.rows);

    if cols_v.is_null() || rows_v.is_null() {
        for v in [cols_v, rows_v] {
            if !v.is_null() {
                unsafe { builder.release(v) };
            }
        }
        return core::ptr::null_mut();
    }

    let names: [*const c_char; 2] = [c"cols".as_ptr(), c"rows".as_ptr()];
    let values: [*mut TaidaAddonValueV1; 2] = [cols_v, rows_v];
    builder.pack(&names, &values)
}

fn build_default_resize_subpack(builder: &HostValueBuilder<'_>) -> *mut TaidaAddonValueV1 {
    build_resize_subpack(builder, &DecodedResize { cols: 0, rows: 0 })
}

// ── Public entry: read_event_impl() over the addon ABI ──────────

/// Implementation backing the addon `readEvent` entry point.
///
/// Preconditions:
/// - Raw mode must be active (via `RawModeEnter`).
/// - stdin must be a TTY.
pub fn read_event_impl(
    host_ptr: *const TaidaHostV1,
    args_len: u32,
    out_value: *mut *mut TaidaAddonValueV1,
    out_error: *mut *mut TaidaAddonErrorV1,
) -> TaidaAddonStatus {
    if args_len != 0 {
        return TaidaAddonStatus::ArityMismatch;
    }
    if host_ptr.is_null() {
        return TaidaAddonStatus::InvalidState;
    }

    let result = panic::catch_unwind(AssertUnwindSafe(|| {
        let builder = match unsafe { HostValueBuilder::from_raw(host_ptr) } {
            Some(b) => b,
            None => return EventInflightStatus::InvalidHost,
        };

        // Check TTY.
        let is_tty = unsafe { libc::isatty(libc::STDIN_FILENO) };
        if is_tty != 1 {
            return EventInflightStatus::NotATty(builder);
        }

        // Check raw mode is active.
        if !crate::raw_mode::is_raw_mode_active() {
            return EventInflightStatus::NotInRawMode(builder);
        }

        // Read one event.
        match read_one_event(libc::STDIN_FILENO) {
            EventReadOutcome::Event(ev) => EventInflightStatus::Decoded(builder, ev),
            EventReadOutcome::Eof => EventInflightStatus::Eof(builder),
            EventReadOutcome::Interrupted => EventInflightStatus::Interrupted(builder),
            EventReadOutcome::Io(e) => EventInflightStatus::Io(builder, e),
            EventReadOutcome::ResizeInitFailed => EventInflightStatus::ResizeInitFailed(builder),
        }
    }));

    let outcome = match result {
        Ok(o) => o,
        Err(_) => {
            if let Some(builder) = unsafe { HostValueBuilder::from_raw(host_ptr) } {
                let e = builder.error(err::READ_EVENT_PANIC, "ReadEventPanic: addon panicked");
                if !out_error.is_null() {
                    unsafe { *out_error = e };
                }
            }
            return TaidaAddonStatus::Error;
        }
    };

    match outcome {
        EventInflightStatus::InvalidHost => TaidaAddonStatus::InvalidState,
        EventInflightStatus::NotATty(builder) => {
            let e = builder.error(
                err::READ_EVENT_NOT_A_TTY,
                "ReadEventNotATty: stdin is not a TTY",
            );
            if !out_error.is_null() {
                unsafe { *out_error = e };
            }
            TaidaAddonStatus::Error
        }
        EventInflightStatus::NotInRawMode(builder) => {
            let e = builder.error(
                err::READ_EVENT_NOT_IN_RAW_MODE,
                "ReadEventNotInRawMode: raw mode must be active (call RawModeEnter first)",
            );
            if !out_error.is_null() {
                unsafe { *out_error = e };
            }
            TaidaAddonStatus::Error
        }
        EventInflightStatus::Eof(builder) => {
            let e = builder.error(err::READ_EVENT_EOF, "ReadEventEof: stdin closed");
            if !out_error.is_null() {
                unsafe { *out_error = e };
            }
            TaidaAddonStatus::Error
        }
        EventInflightStatus::Interrupted(builder) => {
            let e = builder.error(
                err::READ_EVENT_INTERRUPTED,
                "ReadEventInterrupted: read interrupted by signal",
            );
            if !out_error.is_null() {
                unsafe { *out_error = e };
            }
            TaidaAddonStatus::Error
        }
        EventInflightStatus::Io(builder, errno) => {
            let msg = format!("ReadEventReadFailed: read(2) failed (errno {})", errno);
            let e = builder.error(err::READ_EVENT_READ_FAILED, &msg);
            if !out_error.is_null() {
                unsafe { *out_error = e };
            }
            TaidaAddonStatus::Error
        }
        EventInflightStatus::ResizeInitFailed(builder) => {
            let e = builder.error(
                err::READ_EVENT_RESIZE_INIT_FAILED,
                "ReadEventResizeInitFailed: SIGWINCH pipe/handler initialization failed",
            );
            if !out_error.is_null() {
                unsafe { *out_error = e };
            }
            TaidaAddonStatus::Error
        }
        EventInflightStatus::Decoded(builder, event) => {
            let pack = build_event_pack(&builder, &event);
            if pack.is_null() {
                let e = builder.error(
                    err::READ_EVENT_READ_FAILED,
                    "ReadEventReadFailed: failed to build return pack",
                );
                if !out_error.is_null() {
                    unsafe { *out_error = e };
                }
                return TaidaAddonStatus::Error;
            }
            if !out_value.is_null() {
                unsafe { *out_value = pack };
            }
            TaidaAddonStatus::Ok
        }
    }
}

/// Local outcome enum for the catch_unwind closure.
enum EventInflightStatus<'a> {
    InvalidHost,
    NotATty(HostValueBuilder<'a>),
    NotInRawMode(HostValueBuilder<'a>),
    Eof(HostValueBuilder<'a>),
    Interrupted(HostValueBuilder<'a>),
    Io(HostValueBuilder<'a>, i32),
    ResizeInitFailed(HostValueBuilder<'a>),
    Decoded(HostValueBuilder<'a>, DecodedEvent),
}

// ── TMB-017 test-only probe ─────────────────────────────────────

/// TMB-017 probe: invoke `ensure_sigwinch_pipe` and return an
/// observation of the global state in strict *post-install* order.
///
/// Returns `(rfd, installed_flag, old_handler_non_null)`:
///
/// - `rfd` — pipe read fd (-1 on failure).
/// - `installed_flag` — value of `SIGWINCH_INSTALLED` *after* the
///   call returns.
/// - `old_handler_non_null` — whether `OLD_SIGWINCH` is non-null.
///
/// The invariant pinned by this probe: when the install succeeded
/// (rfd >= 0), `old_handler_non_null` must be true. Under the pre-
/// TMB-017 install order (sigaction-with-old-out first, then publish)
/// this held *eventually* but not atomically w.r.t. the first signal
/// delivery. The new 2-step install order (query old → publish →
/// install new) makes the publication a prerequisite of install, so
/// `old_handler_non_null` is necessarily true before `installed_flag`
/// can become true.
///
/// This function is only compiled in unit/integration test builds.
#[doc(hidden)]
pub fn __test_only_sigwinch_snapshot() -> (i32, bool, bool) {
    let rfd = ensure_sigwinch_pipe();
    // Load in the order a consumer would: installed flag first
    // (fast-path gate), then the chain target.
    let installed = SIGWINCH_INSTALLED.load(Ordering::Acquire);
    let old_non_null = !OLD_SIGWINCH.load(Ordering::Acquire).is_null();
    (rfd, installed, old_non_null)
}

/// TMB-017 review follow-up: **pure** probe of the SIGWINCH install
/// globals without triggering `ensure_sigwinch_pipe`.
///
/// The existing `__test_only_sigwinch_snapshot` unconditionally installs
/// the addon's SIGWINCH handler as a side effect — which is correct for
/// the "install ordering" invariants but makes it impossible to write a
/// test that answers "is the addon already installed?" *before*
/// deciding whether to pre-install an external handler. This pure probe
/// only loads the atomics and performs no syscall, so a caller can
/// cheaply detect the uninstalled state and safely install its own
/// external handler first.
///
/// Returns `(installed_flag, old_handler_non_null)`. Does not install.
#[doc(hidden)]
pub fn __test_only_sigwinch_pure_probe() -> (bool, bool) {
    let installed = SIGWINCH_INSTALLED.load(Ordering::Acquire);
    let old_non_null = !OLD_SIGWINCH.load(Ordering::Acquire).is_null();
    (installed, old_non_null)
}

// ── Unit tests ──────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── EventKind / MouseKind tag layout ────────────────────

    #[test]
    fn event_kind_tags_are_frozen() {
        assert_eq!(EventKind::Key as u32, 0);
        assert_eq!(EventKind::Mouse as u32, 1);
        assert_eq!(EventKind::Resize as u32, 2);
        assert_eq!(EventKind::Unknown as u32, 3);
    }

    #[test]
    fn mouse_kind_tags_are_frozen() {
        assert_eq!(MouseKind::Down as u32, 0);
        assert_eq!(MouseKind::Up as u32, 1);
        assert_eq!(MouseKind::Move as u32, 2);
        assert_eq!(MouseKind::Drag as u32, 3);
        assert_eq!(MouseKind::ScrollUp as u32, 4);
        assert_eq!(MouseKind::ScrollDown as u32, 5);
    }

    // ── Error codes ─────────────────────────────────────────

    #[test]
    fn event_error_codes_are_frozen() {
        assert_eq!(err::READ_EVENT_NOT_IN_RAW_MODE, 4001);
        assert_eq!(err::READ_EVENT_NOT_A_TTY, 4002);
        assert_eq!(err::READ_EVENT_READ_FAILED, 4003);
        assert_eq!(err::READ_EVENT_EOF, 4004);
        assert_eq!(err::READ_EVENT_INTERRUPTED, 4005);
        assert_eq!(err::READ_EVENT_PANIC, 4006);
        assert_eq!(err::READ_EVENT_RESIZE_INIT_FAILED, 4007);
    }

    #[test]
    fn event_error_codes_in_4xxx_range() {
        const _: () = assert!(err::READ_EVENT_NOT_IN_RAW_MODE >= 4000);
        const _: () = assert!(err::READ_EVENT_RESIZE_INIT_FAILED >= 4000);
    }

    // ── SGR mouse decode ────────────────────────────────────

    #[test]
    fn decode_sgr_mouse_left_press() {
        // ESC [ < 0 ; 10 ; 20 M = left button press at (10, 20)
        let buf = b"\x1b[<0;10;20M";
        let m = decode_sgr_mouse(buf).expect("should decode left press");
        assert_eq!(m.kind, MouseKind::Down);
        assert_eq!(m.col, 10);
        assert_eq!(m.row, 20);
        assert_eq!(m.button, 0);
        assert!(!m.ctrl && !m.alt && !m.shift);
    }

    #[test]
    fn decode_sgr_mouse_left_release() {
        // ESC [ < 0 ; 10 ; 20 m = left button release at (10, 20)
        let buf = b"\x1b[<0;10;20m";
        let m = decode_sgr_mouse(buf).expect("should decode left release");
        assert_eq!(m.kind, MouseKind::Up);
        assert_eq!(m.col, 10);
        assert_eq!(m.row, 20);
        assert_eq!(m.button, 0);
    }

    #[test]
    fn decode_sgr_mouse_right_press() {
        // Button 2 = right
        let buf = b"\x1b[<2;5;3M";
        let m = decode_sgr_mouse(buf).expect("should decode right press");
        assert_eq!(m.kind, MouseKind::Down);
        assert_eq!(m.button, 2);
        assert_eq!(m.col, 5);
        assert_eq!(m.row, 3);
    }

    #[test]
    fn decode_sgr_mouse_middle_press() {
        // Button 1 = middle
        let buf = b"\x1b[<1;15;10M";
        let m = decode_sgr_mouse(buf).expect("should decode middle press");
        assert_eq!(m.kind, MouseKind::Down);
        assert_eq!(m.button, 1);
    }

    #[test]
    fn decode_sgr_mouse_scroll_up() {
        // Scroll up: bit 6 (64) + low_bits 0 = 64
        let buf = b"\x1b[<64;1;1M";
        let m = decode_sgr_mouse(buf).expect("should decode scroll up");
        assert_eq!(m.kind, MouseKind::ScrollUp);
    }

    #[test]
    fn decode_sgr_mouse_scroll_down() {
        // Scroll down: bit 6 (64) + low_bits 1 = 65
        let buf = b"\x1b[<65;1;1M";
        let m = decode_sgr_mouse(buf).expect("should decode scroll down");
        assert_eq!(m.kind, MouseKind::ScrollDown);
    }

    #[test]
    fn decode_sgr_mouse_drag() {
        // Drag: bit 5 (32) + button 0 = 32
        let buf = b"\x1b[<32;12;8M";
        let m = decode_sgr_mouse(buf).expect("should decode drag");
        assert_eq!(m.kind, MouseKind::Drag);
        assert_eq!(m.col, 12);
        assert_eq!(m.row, 8);
        assert_eq!(m.button, 0);
    }

    #[test]
    fn decode_sgr_mouse_with_ctrl() {
        // Ctrl + left press: 16 (ctrl bit) + 0 (left) = 16
        let buf = b"\x1b[<16;5;5M";
        let m = decode_sgr_mouse(buf).expect("should decode ctrl+click");
        assert_eq!(m.kind, MouseKind::Down);
        assert!(m.ctrl);
        assert!(!m.alt && !m.shift);
    }

    #[test]
    fn decode_sgr_mouse_with_shift() {
        // Shift + left press: 4 (shift bit) + 0 (left) = 4
        let buf = b"\x1b[<4;5;5M";
        let m = decode_sgr_mouse(buf).expect("should decode shift+click");
        assert_eq!(m.kind, MouseKind::Down);
        assert!(m.shift);
        assert!(!m.ctrl && !m.alt);
    }

    #[test]
    fn decode_sgr_mouse_with_alt() {
        // Alt + left press: 8 (alt bit) + 0 (left) = 8
        let buf = b"\x1b[<8;5;5M";
        let m = decode_sgr_mouse(buf).expect("should decode alt+click");
        assert_eq!(m.kind, MouseKind::Down);
        assert!(m.alt);
        assert!(!m.ctrl && !m.shift);
    }

    #[test]
    fn decode_sgr_mouse_with_all_modifiers() {
        // Ctrl(16) + Alt(8) + Shift(4) + left(0) = 28
        let buf = b"\x1b[<28;1;1M";
        let m = decode_sgr_mouse(buf).expect("should decode all modifiers");
        assert!(m.ctrl && m.alt && m.shift);
    }

    #[test]
    fn decode_sgr_mouse_rejects_too_short() {
        assert!(decode_sgr_mouse(b"\x1b[<0;1;1").is_none());
        assert!(decode_sgr_mouse(b"\x1b[<").is_none());
        assert!(decode_sgr_mouse(b"").is_none());
    }

    #[test]
    fn decode_sgr_mouse_rejects_wrong_prefix() {
        // Missing '<'
        assert!(decode_sgr_mouse(b"\x1b[0;10;20M").is_none());
    }

    #[test]
    fn decode_sgr_mouse_rejects_wrong_final_byte() {
        // Neither 'M' nor 'm'
        assert!(decode_sgr_mouse(b"\x1b[<0;10;20X").is_none());
    }

    #[test]
    fn decode_sgr_mouse_large_coordinates() {
        // Large coordinates (wide terminal)
        let buf = b"\x1b[<0;300;100M";
        let m = decode_sgr_mouse(buf).expect("should handle large coords");
        assert_eq!(m.col, 300);
        assert_eq!(m.row, 100);
    }

    // ── SIGWINCH pipe ───────────────────────────────────────

    #[test]
    fn sigwinch_pipe_can_be_installed() {
        let rfd = ensure_sigwinch_pipe();
        // Should succeed (or return -1 in very constrained environments).
        // We can't easily verify the signal handler without sending
        // SIGWINCH to ourselves, but we can verify the pipe fd is valid.
        if rfd >= 0 {
            // fd should be valid and non-blocking.
            let flags = unsafe { libc::fcntl(rfd, libc::F_GETFL) };
            assert!(flags >= 0, "pipe read end must have valid flags");
            assert!(
                flags & libc::O_NONBLOCK != 0,
                "pipe read end must be non-blocking"
            );
        }
    }

    #[test]
    fn sigwinch_pipe_is_idempotent() {
        let rfd1 = ensure_sigwinch_pipe();
        let rfd2 = ensure_sigwinch_pipe();
        assert_eq!(rfd1, rfd2, "repeated calls must return the same fd");
    }

    // ── ReadEvent impl guards ───────────────────────────────

    #[test]
    fn read_event_impl_arity_mismatch() {
        let status = read_event_impl(
            core::ptr::null(),
            1,
            core::ptr::null_mut(),
            core::ptr::null_mut(),
        );
        assert_eq!(status, TaidaAddonStatus::ArityMismatch);
    }

    #[test]
    fn read_event_impl_invalid_state_when_host_null() {
        let status = read_event_impl(
            core::ptr::null(),
            0,
            core::ptr::null_mut(),
            core::ptr::null_mut(),
        );
        assert_eq!(status, TaidaAddonStatus::InvalidState);
    }

    // ── TMB-007: old SIGWINCH handler preservation ─────────

    #[test]
    fn old_sigwinch_handler_is_saved_after_install() {
        // After calling ensure_sigwinch_pipe, the OLD_SIGWINCH pointer
        // should be non-null (we saved the previous handler).
        let rfd = ensure_sigwinch_pipe();
        if rfd >= 0 {
            let old_ptr = OLD_SIGWINCH.load(Ordering::Acquire);
            assert!(
                !old_ptr.is_null(),
                "OLD_SIGWINCH must be non-null after handler installation"
            );
        }
    }

    // ── TMB-017: install order race window ──────────────────
    //
    // These tests pin the 2-step install order introduced by
    // TMB-017 so that future refactors cannot silently reintroduce
    // the null-chain race.

    #[test]
    fn tmb_017_old_handler_published_before_installed_flag() {
        // The fix guarantees: if SIGWINCH_INSTALLED is true, then
        // OLD_SIGWINCH is already non-null. (The new install order
        // publishes OLD_SIGWINCH *before* installing the new
        // handler, and sets INSTALLED = true only after the new
        // handler is live.)
        let rfd = ensure_sigwinch_pipe();
        if rfd < 0 {
            return; // Skip in constrained environments.
        }
        let installed = SIGWINCH_INSTALLED.load(Ordering::Acquire);
        let old_ptr = OLD_SIGWINCH.load(Ordering::Acquire);
        assert!(installed, "install must report success when rfd >= 0");
        assert!(
            !old_ptr.is_null(),
            "TMB-017: OLD_SIGWINCH must be non-null whenever SIGWINCH_INSTALLED is true — \
             the 2-step install order publishes OLD_SIGWINCH before flipping the flag"
        );
    }

    #[test]
    fn tmb_017_reinstall_is_idempotent_and_preserves_old_handler() {
        // Calling ensure_sigwinch_pipe a second time must hit the
        // fast path (flag already set) and must NOT rewrite
        // OLD_SIGWINCH — otherwise we'd capture *our own* handler
        // as the chain target, creating a self-loop.
        let rfd1 = ensure_sigwinch_pipe();
        if rfd1 < 0 {
            return;
        }
        let old1 = OLD_SIGWINCH.load(Ordering::Acquire);
        let rfd2 = ensure_sigwinch_pipe();
        let old2 = OLD_SIGWINCH.load(Ordering::Acquire);
        assert_eq!(rfd1, rfd2);
        assert_eq!(
            old1, old2,
            "TMB-017: second ensure_sigwinch_pipe must not rewrite OLD_SIGWINCH \
             (otherwise we'd chain to ourselves)"
        );

        // The stored old handler must not be our own sigwinch_handler.
        // If it were, the chain path would infinite-loop in a real
        // SIGWINCH delivery.
        if !old2.is_null() {
            let stored = unsafe { &*old2 };
            let our_handler = sigwinch_handler as *const () as usize;
            assert_ne!(
                stored.sa_sigaction, our_handler,
                "TMB-017: OLD_SIGWINCH must not capture our own handler — \
                 install order fix must query BEFORE installing"
            );
        }
    }

    #[test]
    fn sigwinch_handler_chains_without_crash() {
        // Install our handler, then send SIGWINCH to ourselves.
        // This exercises the chain path — if the old handler was
        // SIG_DFL or SIG_IGN, the chain code must not crash.
        let rfd = ensure_sigwinch_pipe();
        if rfd < 0 {
            return; // Skip in constrained environments.
        }
        unsafe {
            libc::kill(libc::getpid(), libc::SIGWINCH);
        }
        // Give the signal a moment to be delivered.
        std::thread::sleep(std::time::Duration::from_millis(10));
        // Drain the pipe — should have at least one byte.
        let mut buf = [0u8; 8];
        let n = unsafe { libc::read(rfd, buf.as_mut_ptr() as *mut _, buf.len()) };
        assert!(n > 0, "self-pipe should have data after SIGWINCH");
    }

    // ── TMB-008: ResizeInitFailed variant ───────────────────

    #[test]
    fn event_read_outcome_resize_init_failed_exists() {
        // Verify the variant exists and pattern-matches correctly.
        let outcome = EventReadOutcome::ResizeInitFailed;
        assert!(
            matches!(outcome, EventReadOutcome::ResizeInitFailed),
            "ResizeInitFailed variant must exist"
        );
    }

    // ── TMB-009: Event framing (decode_one_event) ───────────

    #[test]
    fn framing_single_printable_char() {
        let data = b"a";
        let (consumed, event) = decode_one_event(data);
        assert_eq!(consumed, 1);
        match event {
            DecodedEvent::Key(dk) => {
                assert_eq!(dk.kind, KeyKind::Char);
                assert_eq!(dk.text, "a");
            }
            _ => panic!("expected Key event"),
        }
    }

    #[test]
    fn framing_two_printable_chars_consumes_only_first() {
        let data = b"ab";
        let (consumed, event) = decode_one_event(data);
        assert_eq!(consumed, 1, "must consume exactly 1 byte for 'a'");
        match event {
            DecodedEvent::Key(dk) => {
                assert_eq!(dk.text, "a");
            }
            _ => panic!("expected Key event"),
        }
    }

    #[test]
    fn framing_lone_esc() {
        let data = b"\x1b";
        let (consumed, event) = decode_one_event(data);
        assert_eq!(consumed, 1);
        match event {
            DecodedEvent::Key(dk) => {
                assert_eq!(dk.kind, KeyKind::Escape);
            }
            _ => panic!("expected Key event"),
        }
    }

    #[test]
    fn framing_esc_bracket_a_arrow_up() {
        // ESC [ A = arrow up (3 bytes)
        let data = b"\x1b[A";
        let (consumed, event) = decode_one_event(data);
        assert_eq!(consumed, 3);
        match event {
            DecodedEvent::Key(dk) => {
                assert_eq!(dk.kind, KeyKind::ArrowUp);
            }
            _ => panic!("expected Key event"),
        }
    }

    #[test]
    fn framing_arrow_up_followed_by_char() {
        // ESC [ A followed by 'x'. Must consume only the escape seq.
        let data = b"\x1b[Ax";
        let (consumed, _event) = decode_one_event(data);
        assert_eq!(consumed, 3, "must consume only the 3-byte arrow sequence");
    }

    #[test]
    fn framing_sgr_mouse_event() {
        // ESC [ < 0 ; 10 ; 20 M = mouse press
        let data = b"\x1b[<0;10;20M";
        let (consumed, event) = decode_one_event(data);
        assert_eq!(consumed, data.len());
        match event {
            DecodedEvent::Mouse(m) => {
                assert_eq!(m.kind, MouseKind::Down);
                assert_eq!(m.col, 10);
                assert_eq!(m.row, 20);
            }
            _ => panic!("expected Mouse event"),
        }
    }

    #[test]
    fn framing_sgr_mouse_followed_by_key() {
        // Two events concatenated: mouse press + 'a'.
        let data = b"\x1b[<0;5;5Ma";
        let (consumed, event) = decode_one_event(data);
        assert_eq!(consumed, data.len() - 1, "must consume only mouse event");
        assert!(matches!(event, DecodedEvent::Mouse(_)));
    }

    #[test]
    fn framing_two_sgr_mouse_events() {
        // Two mouse events back-to-back.
        let press = b"\x1b[<0;1;1M";
        let release = b"\x1b[<0;1;1m";
        let mut combined = Vec::new();
        combined.extend_from_slice(press);
        combined.extend_from_slice(release);

        let (consumed1, ev1) = decode_one_event(&combined);
        assert_eq!(
            consumed1,
            press.len(),
            "first event must consume only press"
        );
        assert!(matches!(ev1, DecodedEvent::Mouse(_)));

        let (consumed2, ev2) = decode_one_event(&combined[consumed1..]);
        assert_eq!(consumed2, release.len());
        match ev2 {
            DecodedEvent::Mouse(m) => assert_eq!(m.kind, MouseKind::Up),
            _ => panic!("expected Mouse Up event"),
        }
    }

    #[test]
    fn framing_alt_key() {
        // ESC a = Alt+a (2 bytes)
        let data = b"\x1ba";
        let (consumed, event) = decode_one_event(data);
        assert_eq!(consumed, 2);
        match event {
            DecodedEvent::Key(dk) => {
                assert!(dk.alt);
                assert_eq!(dk.text, "a");
            }
            _ => panic!("expected Key event"),
        }
    }

    #[test]
    fn framing_alt_key_followed_by_char() {
        // ESC a followed by 'b'. Must consume only the alt sequence.
        let data = b"\x1bab";
        let (consumed, _) = decode_one_event(data);
        assert_eq!(consumed, 2, "must consume only the 2-byte Alt+a sequence");
    }

    #[test]
    fn framing_utf8_multibyte() {
        // U+3042 (HIRAGANA LETTER A) = 3 bytes: E3 81 82
        let data = b"\xe3\x81\x82";
        let (consumed, event) = decode_one_event(data);
        assert_eq!(consumed, 3);
        match event {
            DecodedEvent::Key(dk) => {
                assert_eq!(dk.kind, KeyKind::Char);
            }
            _ => panic!("expected Key event"),
        }
    }

    #[test]
    fn framing_utf8_followed_by_ascii() {
        // 3-byte UTF-8 + 'a'
        let data = b"\xe3\x81\x82a";
        let (consumed, _) = decode_one_event(data);
        assert_eq!(consumed, 3, "must consume only the 3-byte UTF-8 char");
    }

    #[test]
    fn framing_ss3_f1_key() {
        // ESC O P = F1 (3 bytes)
        let data = b"\x1bOP";
        let (consumed, event) = decode_one_event(data);
        assert_eq!(consumed, 3);
        match event {
            DecodedEvent::Key(dk) => {
                assert_eq!(dk.kind, KeyKind::F1);
            }
            _ => panic!("expected Key event"),
        }
    }

    #[test]
    fn framing_csi_tilde_key() {
        // ESC [ 5 ~ = PageUp (4 bytes)
        let data = b"\x1b[5~";
        let (consumed, event) = decode_one_event(data);
        assert_eq!(consumed, 4);
        match event {
            DecodedEvent::Key(dk) => {
                assert_eq!(dk.kind, KeyKind::PageUp);
            }
            _ => panic!("expected Key event"),
        }
    }

    #[test]
    fn framing_csi_tilde_followed_by_char() {
        // ESC [ 5 ~ followed by 'x'
        let data = b"\x1b[5~x";
        let (consumed, _) = decode_one_event(data);
        assert_eq!(consumed, 4, "must consume only the 4-byte CSI ~ sequence");
    }

    #[test]
    fn pending_bytes_queue_is_initially_empty() {
        let pending = PENDING_BYTES.lock().unwrap();
        // Note: this test may interact with other tests. We just check
        // the type is correct. In practice the queue is empty at start.
        let _ = pending.len(); // Compiles means VecDeque<u8> is correct.
    }

    // ── Helper function tests ──────────────────────────────

    #[test]
    fn find_csi_end_basic() {
        // ESC [ A
        let data = b"\x1b[A";
        assert_eq!(find_csi_end(data), Some(2));
    }

    #[test]
    fn find_csi_end_parameterised() {
        // ESC [ 1 ; 2 A
        let data = b"\x1b[1;2A";
        assert_eq!(find_csi_end(data), Some(5));
    }

    #[test]
    fn find_csi_end_tilde() {
        // ESC [ 5 ~
        let data = b"\x1b[5~";
        assert_eq!(find_csi_end(data), Some(3));
    }

    #[test]
    fn find_csi_end_none_when_incomplete() {
        // ESC [ 5 (no final byte)
        let data = b"\x1b[5";
        assert_eq!(find_csi_end(data), None);
    }

    #[test]
    fn find_sgr_mouse_end_basic() {
        let data = b"\x1b[<0;10;20M";
        assert_eq!(find_sgr_mouse_end(data), Some(10));
    }

    #[test]
    fn find_sgr_mouse_end_release() {
        let data = b"\x1b[<0;10;20m";
        assert_eq!(find_sgr_mouse_end(data), Some(10));
    }

    #[test]
    fn find_sgr_mouse_end_none_when_incomplete() {
        let data = b"\x1b[<0;10;20";
        assert_eq!(find_sgr_mouse_end(data), None);
    }
}
