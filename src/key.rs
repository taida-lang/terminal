//! `taida-lang/terminal` — `ReadKey[]()` implementation.
//!
//! This module owns the **raw mode lifecycle** mandated by
//! `RC2_DESIGN.md` Section C and `RC2_BLOCKERS.md` RC2B-202.
//!
//! ## Invariants
//!
//! 1. `ReadKey[]()` enters raw mode in a single place (`RawModeGuard::enter`)
//!    and **only** restores via the `Drop` impl. There is no manual
//!    `tcsetattr` branch.
//! 2. The guard is dropped on every exit path — success, EOF, EINTR,
//!    `read` failure, panic — because Rust unwinding runs the destructor.
//! 3. `catch_unwind` wraps the entire `read_key` body so any panic
//!    inside the addon does not unwind across the FFI boundary.
//! 4. Non-TTY stdin is detected **before** entering raw mode, so we
//!    never touch `termios` when stdin is a pipe / file.
//! 5. The function is single-threaded blocking; re-entry from inside
//!    a callback is undefined and not protected against (the Taida
//!    runtime is single-threaded for native addons).
//!
//! ## Key buffer sizing
//!
//! VT100/xterm escape sequences for the keys we identify (arrows, F-keys,
//! navigation block) fit in **at most 6 bytes**. We size the read buffer
//! at 16 bytes to leave headroom for paste-like bursts. Anything that
//! does not match a known prefix is returned as `KeyKind::Unknown` with
//! `text = raw bytes` (silent drop is forbidden).
//!
//! The `#[cfg(unix)]` gate lives on the `mod key;` declaration in
//! `lib.rs`, not here, so we don't shadow it with a redundant inner
//! attribute.

use core::ffi::c_char;
use core::mem::MaybeUninit;
use core::panic::AssertUnwindSafe;
use std::panic;
use std::sync::Mutex;

use taida_addon::bridge::HostValueBuilder;
use taida_addon::{TaidaAddonErrorV1, TaidaAddonStatus, TaidaAddonValueV1, TaidaHostV1};

// ── KeyKind tag table (v1 lock) ──────────────────────────────────
//
// Wire format: u32 integer carried in the `kind` field of the return
// pack. The Taida-side facade re-exports these names as constants and
// users compare against them. Renumbering or reordering requires an
// ABI bump.

/// Frozen `KeyKind` discriminants. The numeric values are part of the
/// surface — the Taida-side facade and host integration tests pin them.
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyKind {
    Char = 0,
    Enter = 1,
    Escape = 2,
    Tab = 3,
    Backspace = 4,
    Delete = 5,
    ArrowUp = 6,
    ArrowDown = 7,
    ArrowLeft = 8,
    ArrowRight = 9,
    Home = 10,
    End = 11,
    PageUp = 12,
    PageDown = 13,
    Insert = 14,
    F1 = 15,
    F2 = 16,
    F3 = 17,
    F4 = 18,
    F5 = 19,
    F6 = 20,
    F7 = 21,
    F8 = 22,
    F9 = 23,
    F10 = 24,
    F11 = 25,
    F12 = 26,
    Unknown = 27,
}

impl KeyKind {
    /// Tag value carried over the bridge.
    pub const fn tag(self) -> i64 {
        self as i64
    }
}

// ── Error variant codes ──────────────────────────────────────────
//
// Wire format: u32 carried in `TaidaAddonErrorV1::code`. The host can
// surface them as deterministic error variant names. The integers are
// part of the surface; renumbering requires an ABI bump.

/// `ReadKey[]()` error variant codes (Section D of RC2_DESIGN.md).
pub mod err {
    pub const READ_KEY_NOT_A_TTY: u32 = 1001;
    pub const READ_KEY_RAW_MODE: u32 = 1002;
    pub const READ_KEY_EOF: u32 = 1003;
    pub const READ_KEY_INTERRUPTED: u32 = 1004;
    pub const READ_KEY_PANIC: u32 = 1005;
    pub const READ_KEY_INVALID_STATE: u32 = 1006;
}

// ── Re-entry guard ───────────────────────────────────────────────
//
// The design forbids `ReadKey[]()` re-entry. We enforce it with a
// process-wide `Mutex` that wraps a single bool. If the lock is already
// held when a call enters, we refuse and return `READ_KEY_INVALID_STATE`
// rather than corrupt the saved termios.

static READ_KEY_INFLIGHT: Mutex<bool> = Mutex::new(false);

struct InflightGuard;

impl InflightGuard {
    fn try_enter() -> Option<Self> {
        let mut g = match READ_KEY_INFLIGHT.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        if *g {
            return None;
        }
        *g = true;
        Some(InflightGuard)
    }
}

impl Drop for InflightGuard {
    fn drop(&mut self) {
        let mut g = match READ_KEY_INFLIGHT.lock() {
            Ok(g) => g,
            // If a previous holder panicked, the mutex is poisoned.
            // We still own the inflight slot logically (Drop runs),
            // so clear it via the poisoned guard.
            Err(p) => p.into_inner(),
        };
        *g = false;
    }
}

// ── RAII raw mode guard ──────────────────────────────────────────
//
// On `enter`, we save the current `termios`, install a `cfmakeraw`d
// copy, and return a `RawModeGuard` that **always** restores on drop —
// success, error, panic. Manual restore is forbidden.

struct RawModeGuard {
    fd: i32,
    saved: libc::termios,
}

impl RawModeGuard {
    /// Enter raw mode on `fd`. Returns `Err(errno)` if either the
    /// initial `tcgetattr` or the subsequent `tcsetattr` fails. On
    /// failure, the terminal is left in its original state (we never
    /// install a partially-initialised termios).
    fn enter(fd: i32) -> Result<Self, i32> {
        // SAFETY: `MaybeUninit::zeroed` produces an all-zero termios; the
        // following `tcgetattr` either fully populates it or fails. We
        // only read from `saved` after a successful tcgetattr.
        let mut saved = MaybeUninit::<libc::termios>::zeroed();
        let rc = unsafe { libc::tcgetattr(fd, saved.as_mut_ptr()) };
        if rc != 0 {
            // RC2.6B-022: portable errno retrieval (macOS uses __error).
            return Err(std::io::Error::last_os_error().raw_os_error().unwrap_or(-1));
        }
        // SAFETY: tcgetattr succeeded → `saved` is fully initialized.
        let saved = unsafe { saved.assume_init() };

        // Build the raw mode termios off a copy so the saved one stays
        // pristine for restore.
        let mut raw = saved;
        // SAFETY: cfmakeraw mutates `raw` in place; `raw` is owned and
        // properly initialised from the prior `saved` copy.
        unsafe { libc::cfmakeraw(&mut raw) };
        // Single-byte read with no inter-byte timer. We do all the
        // sequence assembly ourselves with non-blocking follow-up reads.
        raw.c_cc[libc::VMIN] = 1;
        raw.c_cc[libc::VTIME] = 0;
        let rc = unsafe { libc::tcsetattr(fd, libc::TCSANOW, &raw) };
        if rc != 0 {
            // RC2.6B-022: portable errno retrieval.
            let e = std::io::Error::last_os_error().raw_os_error().unwrap_or(-1);
            // termios was not changed → nothing to restore.
            return Err(e);
        }
        Ok(RawModeGuard { fd, saved })
    }
}

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        // Best-effort restore. We **must not** panic from a Drop impl,
        // and we **must not** loop on EINTR forever (the host process
        // might be shutting down). One retry is enough for typical
        // terminal sessions.
        let mut tries = 0;
        loop {
            let rc = unsafe { libc::tcsetattr(self.fd, libc::TCSANOW, &self.saved) };
            if rc == 0 {
                return;
            }
            // RC2.6B-022: portable errno retrieval.
            let e = std::io::Error::last_os_error().raw_os_error().unwrap_or(-1);
            if e == libc::EINTR && tries < 3 {
                tries += 1;
                continue;
            }
            return;
        }
    }
}

// ── Decoded key ──────────────────────────────────────────────────

/// One decoded key event. Modifier flags only ever flip to `true` for
/// keys whose escape sequence carried a CSI modifier parameter (e.g.
/// `ESC [ 1 ; 5 A` → `ArrowUp` + `ctrl`). For plain `Char` events the
/// `ctrl` flag is set when the byte is in the C0 control range
/// (`0x01..=0x1A`) and maps to a printable letter.
#[derive(Debug, Clone)]
pub struct DecodedKey {
    pub kind: KeyKind,
    pub text: String,
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
}

impl DecodedKey {
    fn plain(kind: KeyKind) -> Self {
        DecodedKey {
            kind,
            text: String::new(),
            ctrl: false,
            alt: false,
            shift: false,
        }
    }

    fn ch(text: String) -> Self {
        DecodedKey {
            kind: KeyKind::Char,
            text,
            ctrl: false,
            alt: false,
            shift: false,
        }
    }

    fn unknown(raw: &[u8]) -> Self {
        // RC2_DESIGN.md B-2: Unknown carries the *raw bytes* in `text`,
        // not silent-drop. Bytes that aren't valid UTF-8 are surfaced
        // via `from_utf8_lossy` so the host always sees a Str payload.
        DecodedKey {
            kind: KeyKind::Unknown,
            text: String::from_utf8_lossy(raw).into_owned(),
            ctrl: false,
            alt: false,
            shift: false,
        }
    }
}

// ── Read result ──────────────────────────────────────────────────

enum ReadOutcome {
    Decoded(DecodedKey),
    Eof,
    Interrupted,
    Io(i32),
}

// ── Decoder (pure, no I/O) ───────────────────────────────────────
//
// The decoder is split out so unit tests can drive it with synthetic
// byte slices — no pty / no stdin needed.

/// Decode a single key event from a buffer. Returns the decoded key.
///
/// Inputs that don't match any known prefix are returned as
/// `KeyKind::Unknown` carrying the raw bytes in `.text` (per
/// RC2_DESIGN.md B-2 — silent drop is forbidden).
pub fn decode(buf: &[u8]) -> DecodedKey {
    if buf.is_empty() {
        // Treated as a degenerate Unknown rather than panicking. The
        // I/O layer never feeds us an empty buffer because `read` of 0
        // bytes is mapped to `Eof`, but the unit tests need a defined
        // contract.
        return DecodedKey::unknown(buf);
    }

    // Single-byte cases.
    if buf.len() == 1 {
        let b = buf[0];
        return decode_single_byte(b);
    }

    // Multi-byte: must start with ESC for any of the sequences we
    // recognise. Anything else with len > 1 is treated as Unknown so
    // we never silently misinterpret raw UTF-8.
    if buf[0] != 0x1B {
        // Try UTF-8 character (1..=4 bytes that decode to a single
        // codepoint).
        if let Some(ch) = utf8_single_char(buf) {
            let mut text = String::new();
            text.push(ch);
            return DecodedKey::ch(text);
        }
        return DecodedKey::unknown(buf);
    }

    // ESC followed by something — try escape-sequence dispatch.
    decode_escape(buf)
}

fn decode_single_byte(b: u8) -> DecodedKey {
    match b {
        b'\r' | b'\n' => DecodedKey::plain(KeyKind::Enter),
        b'\t' => DecodedKey::plain(KeyKind::Tab),
        0x7F | 0x08 => DecodedKey::plain(KeyKind::Backspace),
        0x1B => DecodedKey::plain(KeyKind::Escape),
        // NUL is what the terminal sends for Ctrl+Space (and Ctrl+@).
        // Map it to the space character with `ctrl = true` so it is
        // detectable like every other Ctrl chord.
        0x00 => DecodedKey {
            kind: KeyKind::Char,
            text: " ".to_string(),
            ctrl: true,
            alt: false,
            shift: false,
        },
        // C0 control range — Ctrl+letter. We map them to Char with the
        // canonical letter and `ctrl = true` so users can detect e.g.
        // Ctrl-C without parsing the byte themselves.
        0x01..=0x1A => {
            let mut text = String::new();
            text.push((b - 1 + b'a') as char);
            DecodedKey {
                kind: KeyKind::Char,
                text,
                ctrl: true,
                alt: false,
                shift: false,
            }
        }
        // The C0 bytes above the letter range are the Ctrl+punctuation
        // chords: FS/GS/RS/US = Ctrl+\\ Ctrl+] Ctrl+^ Ctrl+_ (their
        // codepoints sit exactly 0x40 below the punctuation).
        0x1C..=0x1F => {
            let mut text = String::new();
            text.push((b + 0x40) as char);
            DecodedKey {
                kind: KeyKind::Char,
                text,
                ctrl: true,
                alt: false,
                shift: false,
            }
        }
        // Printable ASCII.
        0x20..=0x7E => {
            let mut text = String::new();
            text.push(b as char);
            DecodedKey::ch(text)
        }
        // High byte alone — leading byte of a multi-byte UTF-8
        // sequence we never finished. Surface as Unknown.
        _ => DecodedKey::unknown(&[b]),
    }
}

/// Try to decode `buf` as a single UTF-8 codepoint. Returns the char if
/// the entire buffer is exactly one codepoint, otherwise `None`.
fn utf8_single_char(buf: &[u8]) -> Option<char> {
    let s = core::str::from_utf8(buf).ok()?;
    let mut iter = s.chars();
    let first = iter.next()?;
    if iter.next().is_some() {
        return None;
    }
    Some(first)
}

/// Decode an escape sequence (`buf[0] == 0x1B`).
fn decode_escape(buf: &[u8]) -> DecodedKey {
    debug_assert!(buf.first() == Some(&0x1B));
    if buf.len() == 1 {
        return DecodedKey::plain(KeyKind::Escape);
    }
    match buf[1] {
        b'[' => decode_csi(buf),
        b'O' => decode_ss3(buf),
        // ESC ESC — an Alt-prefixed sequence. Many terminals send
        // Alt+Arrow (and friends) as a second ESC in front of the
        // whole sequence; decode the inner sequence and overlay the
        // alt flag. A bare ESC ESC pair decodes as Alt+Escape.
        0x1B => {
            let mut inner = decode(&buf[1..]);
            inner.alt = true;
            inner
        }
        // ESC + printable byte → Alt + that byte (Char with alt=true).
        c if (0x20..=0x7E).contains(&c) => {
            let mut text = String::new();
            text.push(c as char);
            DecodedKey {
                kind: KeyKind::Char,
                text,
                ctrl: false,
                alt: true,
                shift: false,
            }
        }
        _ => DecodedKey::unknown(buf),
    }
}

/// CSI (`ESC [ ...`). Recognises:
///
/// - Arrow keys: `ESC [ A/B/C/D` and modifier form `ESC [ 1 ; m A`
/// - Home/End: `ESC [ H`, `ESC [ F`, `ESC [ 1 ~`, `ESC [ 4 ~`
/// - PageUp/PageDown: `ESC [ 5 ~`, `ESC [ 6 ~`
/// - Insert/Delete: `ESC [ 2 ~`, `ESC [ 3 ~`
/// - Function keys via `ESC [ <n> ~` table
fn decode_csi(buf: &[u8]) -> DecodedKey {
    debug_assert!(buf.len() >= 2 && buf[1] == b'[');
    if buf.len() < 3 {
        return DecodedKey::unknown(buf);
    }

    // Two forms:
    //  (a) ESC [ <letter>                       — single-letter final
    //  (b) ESC [ <params> <letter or '~'>       — parameterised
    //
    // For (b) we collect digits + ';' until we see a non-digit non-';'.
    // The final byte determines the key; semicolon-separated parameters
    // carry the modifier mask.
    let mut params: [u32; 4] = [0; 4];
    let mut nparams = 0usize;
    let mut cur: u32 = 0;
    let mut have_digits = false;
    let mut i = 2usize;

    while i < buf.len() {
        let b = buf[i];
        match b {
            b'0'..=b'9' => {
                cur = cur.saturating_mul(10).saturating_add((b - b'0') as u32);
                have_digits = true;
                i += 1;
            }
            b';' => {
                if nparams < params.len() {
                    params[nparams] = if have_digits { cur } else { 0 };
                    nparams += 1;
                }
                cur = 0;
                have_digits = false;
                i += 1;
            }
            _ => break,
        }
    }
    if i >= buf.len() {
        return DecodedKey::unknown(buf);
    }
    // Push trailing param if we collected digits before the final byte.
    if have_digits && nparams < params.len() {
        params[nparams] = cur;
        nparams += 1;
    }
    let final_byte = buf[i];

    // Modifier mask is always the second parameter when present:
    //   ESC [ 1 ; <m> A    — arrow + modifier (params = [1, m])
    //   ESC [ <n> ; <m> ~  — tilde form + modifier (params = [n, m])
    // When no second parameter is present we have no modifier.
    let modifier_param = if nparams >= 2 { params[1] } else { 0 };
    let (ctrl, alt, shift) = decode_modifier_mask(modifier_param);

    let kind = match final_byte {
        b'A' => Some(KeyKind::ArrowUp),
        b'B' => Some(KeyKind::ArrowDown),
        b'C' => Some(KeyKind::ArrowRight),
        b'D' => Some(KeyKind::ArrowLeft),
        b'H' => Some(KeyKind::Home),
        b'F' => Some(KeyKind::End),
        b'P' => Some(KeyKind::F1),
        b'Q' => Some(KeyKind::F2),
        b'R' => Some(KeyKind::F3),
        b'S' => Some(KeyKind::F4),
        b'~' if nparams >= 1 => match params[0] {
            1 => Some(KeyKind::Home),
            2 => Some(KeyKind::Insert),
            3 => Some(KeyKind::Delete),
            4 => Some(KeyKind::End),
            5 => Some(KeyKind::PageUp),
            6 => Some(KeyKind::PageDown),
            7 => Some(KeyKind::Home),
            8 => Some(KeyKind::End),
            11 => Some(KeyKind::F1),
            12 => Some(KeyKind::F2),
            13 => Some(KeyKind::F3),
            14 => Some(KeyKind::F4),
            15 => Some(KeyKind::F5),
            17 => Some(KeyKind::F6),
            18 => Some(KeyKind::F7),
            19 => Some(KeyKind::F8),
            20 => Some(KeyKind::F9),
            21 => Some(KeyKind::F10),
            23 => Some(KeyKind::F11),
            24 => Some(KeyKind::F12),
            _ => None,
        },
        _ => None,
    };

    match kind {
        Some(k) => DecodedKey {
            kind: k,
            text: String::new(),
            ctrl,
            alt,
            shift,
        },
        None => DecodedKey::unknown(buf),
    }
}

/// SS3 (`ESC O ...`). xterm uses this for arrows in application mode
/// and for F1..F4: `ESC O P/Q/R/S`.
fn decode_ss3(buf: &[u8]) -> DecodedKey {
    debug_assert!(buf.len() >= 2 && buf[1] == b'O');
    if buf.len() < 3 {
        return DecodedKey::unknown(buf);
    }
    let kind = match buf[2] {
        b'A' => KeyKind::ArrowUp,
        b'B' => KeyKind::ArrowDown,
        b'C' => KeyKind::ArrowRight,
        b'D' => KeyKind::ArrowLeft,
        b'H' => KeyKind::Home,
        b'F' => KeyKind::End,
        b'P' => KeyKind::F1,
        b'Q' => KeyKind::F2,
        b'R' => KeyKind::F3,
        b'S' => KeyKind::F4,
        _ => return DecodedKey::unknown(buf),
    };
    DecodedKey::plain(kind)
}

/// xterm modifier mask: `m - 1` is a bitfield of (Shift, Alt, Ctrl, Meta).
/// We surface Shift / Alt / Ctrl. Meta is folded into Alt for parity
/// with common terminals.
fn decode_modifier_mask(m: u32) -> (bool, bool, bool) {
    if m == 0 {
        return (false, false, false);
    }
    let bits = m.saturating_sub(1);
    let shift = bits & 0b0001 != 0;
    let alt = bits & 0b0010 != 0;
    let ctrl = bits & 0b0100 != 0;
    let meta = bits & 0b1000 != 0;
    (ctrl, alt || meta, shift)
}

// ── I/O layer ────────────────────────────────────────────────────

/// Read up to `MAX` bytes of one key event from `fd`. The first byte is
/// blocking (`VMIN=1`); subsequent bytes are read with a short
/// non-blocking poll so we can assemble multi-byte escape sequences
/// without hanging on a stale `ESC` press.
const MAX_KEY_BYTES: usize = 16;

fn read_one_key_native(fd: i32) -> ReadOutcome {
    let mut buf = [0u8; MAX_KEY_BYTES];
    // First read is blocking — VMIN=1 was set by RawModeGuard.
    let n = unsafe { libc::read(fd, buf.as_mut_ptr() as *mut _, 1) };
    if n < 0 {
        // RC2.6B-022: portable errno retrieval.
        let e = std::io::Error::last_os_error().raw_os_error().unwrap_or(-1);
        if e == libc::EINTR {
            return ReadOutcome::Interrupted;
        }
        return ReadOutcome::Io(e);
    }
    if n == 0 {
        return ReadOutcome::Eof;
    }

    let mut len = n as usize;

    // If the first byte is ESC, assemble exactly *one* escape
    // sequence, stopping at its structural end — the same framing
    // `event::read_stdin_event` uses (TMB-009).
    //
    // The previous code drained greedily until the 50 ms poll lapsed.
    // Key auto-repeat (typically ~33 ms) beats that timeout, so a
    // held arrow key concatenated several sequences into one buffer;
    // `decode` interprets only the first and silently dropped the
    // rest. Worse, the 16-byte cap could split a sequence so its tail
    // surfaced as garbage `Char('[')` / `Char('A')` events on later
    // calls.
    if buf[0] == 0x1B {
        // One byte to distinguish a lone ESC from a sequence prefix.
        if let Some(b) = poll_read_one(fd) {
            buf[len] = b;
            len += 1;
        }
        // Alt-prefixed sequence: terminals send Alt+Arrow as a second
        // ESC in front of the whole sequence (`ESC ESC [ A`). Shift
        // the framing origin so the inner sequence is assembled below
        // exactly like an unprefixed one — without this the pair
        // decoded as Unknown and the tail surfaced as garbage
        // `Char('[')` / `Char('A')` events on later calls.
        let mut seq = 1usize;
        if len == 2 && buf[1] == 0x1B {
            if let Some(b) = poll_read_one(fd) {
                buf[len] = b;
                len += 1;
            }
            seq = 2;
        }
        if len > seq && buf[seq] == b'[' {
            // CSI: read until the final byte (0x40..=0x7E), which can
            // only appear past the introducer (covers `ESC [ 1 ; 5 A`
            // modifier forms and SGR mouse `ESC [ < ... M/m`).
            while len < buf.len() {
                let last = buf[len - 1];
                if len >= seq + 2 && (0x40..=0x7E).contains(&last) {
                    break;
                }
                match poll_read_one(fd) {
                    Some(b) => {
                        buf[len] = b;
                        len += 1;
                    }
                    None => break,
                }
            }
            // X10 legacy mouse: a bare `ESC [ M` final (no parameter
            // bytes) is followed by exactly three payload bytes
            // (button, x, y — not final-byte terminated). Consume
            // them with the sequence so they cannot surface as
            // garbage Char events. SGR mouse (`ESC [ < ... M`) has
            // parameter bytes and never matches this shape; `decode`
            // reports the X10 frame as one Unknown event.
            if len == seq + 2 && buf[seq + 1] == b'M' {
                let target = (len + 3).min(buf.len());
                while len < target {
                    match poll_read_one(fd) {
                        Some(b) => {
                            buf[len] = b;
                            len += 1;
                        }
                        None => break,
                    }
                }
            }
            // Oversized CSI: the frame filled before the final byte
            // arrived. Drain the remainder up to the final byte so
            // the tail cannot surface as garbage Char events on later
            // calls — the truncated frame itself decodes as one
            // Unknown. Bounded so a hostile unterminated stream
            // cannot spin here forever.
            if len == buf.len() && !(0x40..=0x7E).contains(&buf[len - 1]) {
                let mut drained = 0usize;
                while drained < 64 {
                    match poll_read_one(fd) {
                        Some(b) if (0x40..=0x7E).contains(&b) => break,
                        Some(_) => drained += 1,
                        None => break,
                    }
                }
            }
        } else if len > seq && buf[seq] == b'O' {
            // SS3: exactly one more byte (e.g. `ESC O P` for F1).
            if len == seq + 1
                && let Some(b) = poll_read_one(fd)
            {
                buf[len] = b;
                len += 1;
            }
        } else if seq == 1 && len >= 2 && buf[1] >= 0x80 {
            // Alt + multi-byte UTF-8: pull the continuation bytes so
            // the character is not split across reads. (`decode`
            // reports this as Unknown, but as *one* event.)
            let expect = utf8_continuation_count(buf[1]);
            let target = (2 + expect).min(buf.len());
            while len < target {
                match poll_read_one(fd) {
                    Some(b) => {
                        buf[len] = b;
                        len += 1;
                    }
                    None => break,
                }
            }
        }
        // Anything else after ESC is a complete 2-byte Alt+key form.
    } else if buf[0] >= 0x80 {
        // Multi-byte UTF-8 leading byte: pull the continuation bytes.
        let expect = utf8_continuation_count(buf[0]);
        let target = (1 + expect).min(buf.len());
        while len < target {
            match poll_read_one(fd) {
                Some(b) => {
                    buf[len] = b;
                    len += 1;
                }
                None => break,
            }
        }
    }

    ReadOutcome::Decoded(decode(&buf[..len]))
}

/// Poll `fd` for up to 50 ms and read exactly one byte.
fn poll_read_one(fd: i32) -> Option<u8> {
    let mut pfd = libc::pollfd {
        fd,
        events: libc::POLLIN,
        revents: 0,
    };
    let p = unsafe { libc::poll(&mut pfd, 1, 50) };
    if p <= 0 {
        return None;
    }
    let mut b: u8 = 0;
    let n = unsafe { libc::read(fd, &mut b as *mut u8 as *mut _, 1) };
    if n <= 0 { None } else { Some(b) }
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

// ── Public entry: read_key() over the addon ABI ──────────────────

/// Build the return pack for a successful read.
fn build_pack(builder: &HostValueBuilder<'_>, decoded: DecodedKey) -> *mut TaidaAddonValueV1 {
    let host = builder.as_raw();
    let kind_v = unsafe { ((*host).value_new_int)(host, decoded.kind.tag()) };
    let text_v =
        unsafe { ((*host).value_new_str)(host, decoded.text.as_ptr(), decoded.text.len()) };
    let ctrl_v = unsafe { ((*host).value_new_bool)(host, u8::from(decoded.ctrl)) };
    let alt_v = unsafe { ((*host).value_new_bool)(host, u8::from(decoded.alt)) };
    let shift_v = unsafe { ((*host).value_new_bool)(host, u8::from(decoded.shift)) };

    if kind_v.is_null()
        || text_v.is_null()
        || ctrl_v.is_null()
        || alt_v.is_null()
        || shift_v.is_null()
    {
        for v in [kind_v, text_v, ctrl_v, alt_v, shift_v] {
            if !v.is_null() {
                unsafe { ((*host).value_release)(host, v) };
            }
        }
        return core::ptr::null_mut();
    }

    // Field order is part of the v1 lock: kind, text, ctrl, alt, shift.
    let kind_name = c"kind";
    let text_name = c"text";
    let ctrl_name = c"ctrl";
    let alt_name = c"alt";
    let shift_name = c"shift";
    let names: [*const c_char; 5] = [
        kind_name.as_ptr(),
        text_name.as_ptr(),
        ctrl_name.as_ptr(),
        alt_name.as_ptr(),
        shift_name.as_ptr(),
    ];
    let values: [*mut TaidaAddonValueV1; 5] = [kind_v, text_v, ctrl_v, alt_v, shift_v];
    crate::pack_util::pack_or_release(builder, &names, &values)
}

/// Implementation backing the addon `readKey` entry point. The C ABI
/// wrapper in `lib.rs` forwards `args_ptr` / `args_len` / `out_value` /
/// `out_error` directly to here after the host has captured the host
/// pointer via `terminal_init`.
pub fn read_key_impl(
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

    // catch_unwind so a Rust panic inside the addon never unwinds
    // across the FFI boundary. AssertUnwindSafe is justified because
    // the only mutable state is the inflight guard (which we drop
    // before recovering) and the termios guard (RAII).
    let result = panic::catch_unwind(AssertUnwindSafe(|| {
        // Re-entry guard.
        let _inflight = match InflightGuard::try_enter() {
            Some(g) => g,
            None => {
                return InflightStatus::Reentered;
            }
        };

        // Build the host wrapper. We re-validate non-null even though
        // the caller already checked, because we can't trust the
        // pointer to remain stable if the host misbehaves.
        let builder = match unsafe { HostValueBuilder::from_raw(host_ptr) } {
            Some(b) => b,
            None => return InflightStatus::InvalidHost,
        };

        // Non-TTY check **before** entering raw mode. Per the design
        // we must never touch termios when stdin is a pipe / file.
        let is_tty = unsafe { libc::isatty(libc::STDIN_FILENO) };
        if is_tty != 1 {
            return InflightStatus::NotATty(builder);
        }

        // TM-2e: If standalone raw mode is active (via RawModeEnter),
        // skip the per-call raw mode enter/leave cycle and just read.
        // This resolves TMB-005 (raw mode state conflict).
        if crate::raw_mode::is_raw_mode_active() {
            return match read_one_key_native(libc::STDIN_FILENO) {
                ReadOutcome::Decoded(d) => InflightStatus::Decoded(builder, d),
                ReadOutcome::Eof => InflightStatus::Eof(builder),
                ReadOutcome::Interrupted => InflightStatus::Interrupted(builder),
                ReadOutcome::Io(e) => InflightStatus::Io(builder, e),
            };
        }

        // Enter raw mode (RAII guard restores on every exit path).
        let _guard = match RawModeGuard::enter(libc::STDIN_FILENO) {
            Ok(g) => g,
            Err(e) => return InflightStatus::RawModeError(builder, e),
        };

        // Read & decode.
        match read_one_key_native(libc::STDIN_FILENO) {
            ReadOutcome::Decoded(d) => InflightStatus::Decoded(builder, d),
            ReadOutcome::Eof => InflightStatus::Eof(builder),
            ReadOutcome::Interrupted => InflightStatus::Interrupted(builder),
            ReadOutcome::Io(e) => InflightStatus::Io(builder, e),
        }
        // _guard dropped here → terminal restored.
    }));

    let outcome = match result {
        Ok(o) => o,
        Err(_payload) => {
            // The catch_unwind body itself panicked. We need a host
            // builder to construct the error, but we may not have one
            // (e.g. panic happened inside HostValueBuilder::from_raw).
            // Try to acquire a fresh one; if even that fails, return
            // a bare Error status with no out_error.
            if let Some(builder) = unsafe { HostValueBuilder::from_raw(host_ptr) } {
                let err = builder.error(err::READ_KEY_PANIC, "ReadKeyPanic: addon panicked");
                if !out_error.is_null() {
                    unsafe { *out_error = err };
                }
            }
            return TaidaAddonStatus::Error;
        }
    };

    match outcome {
        InflightStatus::Reentered => {
            if let Some(builder) = unsafe { HostValueBuilder::from_raw(host_ptr) } {
                let err = builder.error(
                    err::READ_KEY_INVALID_STATE,
                    "ReadKeyInvalidState: re-entrant ReadKey[]() call",
                );
                if !out_error.is_null() {
                    unsafe { *out_error = err };
                }
            }
            TaidaAddonStatus::InvalidState
        }
        InflightStatus::InvalidHost => TaidaAddonStatus::InvalidState,
        InflightStatus::NotATty(builder) => {
            let err = builder.error(
                err::READ_KEY_NOT_A_TTY,
                "ReadKeyNotATty: stdin is not a TTY",
            );
            if !out_error.is_null() {
                unsafe { *out_error = err };
            }
            TaidaAddonStatus::Error
        }
        InflightStatus::RawModeError(builder, e) => {
            let msg = format!("ReadKeyRawMode: failed to enter raw mode (errno {})", e);
            let err = builder.error(err::READ_KEY_RAW_MODE, &msg);
            if !out_error.is_null() {
                unsafe { *out_error = err };
            }
            TaidaAddonStatus::Error
        }
        InflightStatus::Eof(builder) => {
            let err = builder.error(err::READ_KEY_EOF, "ReadKeyEof: stdin closed");
            if !out_error.is_null() {
                unsafe { *out_error = err };
            }
            TaidaAddonStatus::Error
        }
        InflightStatus::Interrupted(builder) => {
            let err = builder.error(
                err::READ_KEY_INTERRUPTED,
                "ReadKeyInterrupted: read interrupted by signal",
            );
            if !out_error.is_null() {
                unsafe { *out_error = err };
            }
            TaidaAddonStatus::Error
        }
        InflightStatus::Io(builder, e) => {
            let msg = format!("ReadKeyRawMode: read failed (errno {})", e);
            let err = builder.error(err::READ_KEY_RAW_MODE, &msg);
            if !out_error.is_null() {
                unsafe { *out_error = err };
            }
            TaidaAddonStatus::Error
        }
        InflightStatus::Decoded(builder, decoded) => {
            let pack = build_pack(&builder, decoded);
            if pack.is_null() {
                let err = builder.error(
                    err::READ_KEY_RAW_MODE,
                    "ReadKeyRawMode: failed to build return pack",
                );
                if !out_error.is_null() {
                    unsafe { *out_error = err };
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

// Local outcome enum so we can carry the (already-built) HostBuilder
// out of the catch_unwind closure without using more catch_unwind
// boilerplate. The builder must outlive the raw mode guard so we can
// build the success pack while raw mode is still active is
// unnecessary — restoring before building is fine because the host
// allocator does not depend on terminal state.
enum InflightStatus<'a> {
    Reentered,
    InvalidHost,
    NotATty(HostValueBuilder<'a>),
    RawModeError(HostValueBuilder<'a>, i32),
    Eof(HostValueBuilder<'a>),
    Interrupted(HostValueBuilder<'a>),
    Io(HostValueBuilder<'a>, i32),
    Decoded(HostValueBuilder<'a>, DecodedKey),
}

// ── Unit tests ───────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── KeyKind tag layout ───────────────────────────────────

    #[test]
    fn key_kind_tags_are_frozen_v1() {
        // The Taida-side facade and host-side parity tests pin these
        // numeric values. Reordering or renumbering requires an ABI
        // bump (RC2_DESIGN.md B-2 v1 lock).
        assert_eq!(KeyKind::Char as u32, 0);
        assert_eq!(KeyKind::Enter as u32, 1);
        assert_eq!(KeyKind::Escape as u32, 2);
        assert_eq!(KeyKind::Tab as u32, 3);
        assert_eq!(KeyKind::Backspace as u32, 4);
        assert_eq!(KeyKind::Delete as u32, 5);
        assert_eq!(KeyKind::ArrowUp as u32, 6);
        assert_eq!(KeyKind::ArrowDown as u32, 7);
        assert_eq!(KeyKind::ArrowLeft as u32, 8);
        assert_eq!(KeyKind::ArrowRight as u32, 9);
        assert_eq!(KeyKind::Home as u32, 10);
        assert_eq!(KeyKind::End as u32, 11);
        assert_eq!(KeyKind::PageUp as u32, 12);
        assert_eq!(KeyKind::PageDown as u32, 13);
        assert_eq!(KeyKind::Insert as u32, 14);
        assert_eq!(KeyKind::F1 as u32, 15);
        assert_eq!(KeyKind::F12 as u32, 26);
        assert_eq!(KeyKind::Unknown as u32, 27);
    }

    // ── Single-byte decoding ─────────────────────────────────

    #[test]
    fn decode_printable_ascii_yields_char() {
        let d = decode(b"a");
        assert_eq!(d.kind, KeyKind::Char);
        assert_eq!(d.text, "a");
        assert!(!d.ctrl && !d.alt && !d.shift);
    }

    #[test]
    fn decode_enter_lf_and_cr() {
        assert_eq!(decode(b"\n").kind, KeyKind::Enter);
        assert_eq!(decode(b"\r").kind, KeyKind::Enter);
    }

    #[test]
    fn decode_tab() {
        assert_eq!(decode(b"\t").kind, KeyKind::Tab);
    }

    #[test]
    fn decode_backspace_both_codes() {
        assert_eq!(decode(b"\x7f").kind, KeyKind::Backspace);
        assert_eq!(decode(b"\x08").kind, KeyKind::Backspace);
    }

    #[test]
    fn decode_lone_escape_is_escape_key() {
        let d = decode(b"\x1b");
        assert_eq!(d.kind, KeyKind::Escape);
    }

    #[test]
    fn decode_ctrl_letter_sets_ctrl_flag() {
        // Ctrl-A = 0x01 → 'a' + ctrl
        let d = decode(&[0x01]);
        assert_eq!(d.kind, KeyKind::Char);
        assert_eq!(d.text, "a");
        assert!(d.ctrl);
        assert!(!d.alt && !d.shift);
        // Ctrl-C = 0x03 → 'c' + ctrl
        let d = decode(&[0x03]);
        assert_eq!(d.text, "c");
        assert!(d.ctrl);
    }

    // ── UTF-8 multi-byte ─────────────────────────────────────

    #[test]
    fn decode_utf8_multibyte_char() {
        // 「あ」 = U+3042 in UTF-8 = 0xE3 0x81 0x82
        let d = decode("あ".as_bytes());
        assert_eq!(d.kind, KeyKind::Char);
        assert_eq!(d.text, "あ");
        assert!(!d.ctrl && !d.alt && !d.shift);
    }

    // ── Escape sequences: arrows ─────────────────────────────

    #[test]
    fn decode_arrow_up_csi() {
        let d = decode(b"\x1b[A");
        assert_eq!(d.kind, KeyKind::ArrowUp);
        assert!(!d.ctrl);
    }

    #[test]
    fn decode_arrow_down_csi() {
        assert_eq!(decode(b"\x1b[B").kind, KeyKind::ArrowDown);
    }

    #[test]
    fn decode_arrow_right_csi() {
        assert_eq!(decode(b"\x1b[C").kind, KeyKind::ArrowRight);
    }

    #[test]
    fn decode_arrow_left_csi() {
        assert_eq!(decode(b"\x1b[D").kind, KeyKind::ArrowLeft);
    }

    #[test]
    fn decode_arrow_up_with_ctrl_modifier() {
        // ESC [ 1 ; 5 A = ArrowUp + Ctrl (xterm modifier mask 5 = ctrl)
        let d = decode(b"\x1b[1;5A");
        assert_eq!(d.kind, KeyKind::ArrowUp);
        assert!(d.ctrl);
        assert!(!d.alt && !d.shift);
    }

    #[test]
    fn decode_arrow_up_with_shift_modifier() {
        // ESC [ 1 ; 2 A = ArrowUp + Shift
        let d = decode(b"\x1b[1;2A");
        assert_eq!(d.kind, KeyKind::ArrowUp);
        assert!(d.shift);
        assert!(!d.ctrl && !d.alt);
    }

    #[test]
    fn decode_arrow_up_with_alt_modifier() {
        // ESC [ 1 ; 3 A = ArrowUp + Alt
        let d = decode(b"\x1b[1;3A");
        assert_eq!(d.kind, KeyKind::ArrowUp);
        assert!(d.alt);
        assert!(!d.ctrl && !d.shift);
    }

    #[test]
    fn decode_arrow_up_with_ctrl_shift_modifier() {
        // ESC [ 1 ; 6 A = ArrowUp + Ctrl + Shift
        let d = decode(b"\x1b[1;6A");
        assert_eq!(d.kind, KeyKind::ArrowUp);
        assert!(d.ctrl);
        assert!(d.shift);
        assert!(!d.alt);
    }

    // ── Escape sequences: navigation block ───────────────────

    #[test]
    fn decode_home_letter_form() {
        assert_eq!(decode(b"\x1b[H").kind, KeyKind::Home);
    }

    #[test]
    fn decode_end_letter_form() {
        assert_eq!(decode(b"\x1b[F").kind, KeyKind::End);
    }

    #[test]
    fn decode_home_tilde_form() {
        // ESC [ 1 ~  = Home (vt220 style)
        assert_eq!(decode(b"\x1b[1~").kind, KeyKind::Home);
        // ESC [ 7 ~  = Home (rxvt style)
        assert_eq!(decode(b"\x1b[7~").kind, KeyKind::Home);
    }

    #[test]
    fn decode_end_tilde_form() {
        assert_eq!(decode(b"\x1b[4~").kind, KeyKind::End);
        assert_eq!(decode(b"\x1b[8~").kind, KeyKind::End);
    }

    #[test]
    fn decode_pageup_pagedown() {
        assert_eq!(decode(b"\x1b[5~").kind, KeyKind::PageUp);
        assert_eq!(decode(b"\x1b[6~").kind, KeyKind::PageDown);
    }

    #[test]
    fn decode_insert_delete() {
        assert_eq!(decode(b"\x1b[2~").kind, KeyKind::Insert);
        assert_eq!(decode(b"\x1b[3~").kind, KeyKind::Delete);
    }

    // ── Escape sequences: F-keys ─────────────────────────────

    #[test]
    fn decode_f1_to_f4_csi_letter_form() {
        assert_eq!(decode(b"\x1b[P").kind, KeyKind::F1);
        assert_eq!(decode(b"\x1b[Q").kind, KeyKind::F2);
        assert_eq!(decode(b"\x1b[R").kind, KeyKind::F3);
        assert_eq!(decode(b"\x1b[S").kind, KeyKind::F4);
    }

    #[test]
    fn decode_f1_to_f4_ss3_form() {
        // xterm application keypad: ESC O P/Q/R/S
        assert_eq!(decode(b"\x1bOP").kind, KeyKind::F1);
        assert_eq!(decode(b"\x1bOQ").kind, KeyKind::F2);
        assert_eq!(decode(b"\x1bOR").kind, KeyKind::F3);
        assert_eq!(decode(b"\x1bOS").kind, KeyKind::F4);
    }

    #[test]
    fn decode_f5_to_f12_tilde_form() {
        assert_eq!(decode(b"\x1b[15~").kind, KeyKind::F5);
        assert_eq!(decode(b"\x1b[17~").kind, KeyKind::F6);
        assert_eq!(decode(b"\x1b[18~").kind, KeyKind::F7);
        assert_eq!(decode(b"\x1b[19~").kind, KeyKind::F8);
        assert_eq!(decode(b"\x1b[20~").kind, KeyKind::F9);
        assert_eq!(decode(b"\x1b[21~").kind, KeyKind::F10);
        assert_eq!(decode(b"\x1b[23~").kind, KeyKind::F11);
        assert_eq!(decode(b"\x1b[24~").kind, KeyKind::F12);
    }

    // ── Alt + key ────────────────────────────────────────────

    #[test]
    fn decode_alt_plus_letter() {
        // ESC + 'a' = Alt-A
        let d = decode(b"\x1ba");
        assert_eq!(d.kind, KeyKind::Char);
        assert_eq!(d.text, "a");
        assert!(d.alt);
        assert!(!d.ctrl && !d.shift);
    }

    // ── Unknown / fallback ───────────────────────────────────

    #[test]
    fn decode_unknown_csi_sequence_preserves_raw_bytes() {
        // ESC [ 99 z — not in our table.
        let d = decode(b"\x1b[99z");
        assert_eq!(d.kind, KeyKind::Unknown);
        assert_eq!(d.text, "\u{1b}[99z");
        assert!(!d.ctrl && !d.alt && !d.shift);
    }

    #[test]
    fn decode_garbage_high_byte_alone_is_unknown_not_silent_drop() {
        // 0x80 alone is invalid UTF-8 → must surface as Unknown
        // (silent drop is forbidden by RC2_DESIGN.md B-2).
        let d = decode(&[0x80]);
        assert_eq!(d.kind, KeyKind::Unknown);
        assert!(!d.text.is_empty());
    }

    #[test]
    fn decode_partial_csi_is_unknown() {
        // Just "ESC [" with nothing after — not a complete sequence.
        let d = decode(b"\x1b[");
        assert_eq!(d.kind, KeyKind::Unknown);
    }

    #[test]
    fn decode_empty_buffer_is_unknown() {
        // The I/O layer maps zero-byte reads to ReadOutcome::Eof, but
        // the decoder must still have a defined contract for empty
        // input so we can unit-test it independently.
        let d = decode(&[]);
        assert_eq!(d.kind, KeyKind::Unknown);
        assert!(d.text.is_empty());
    }

    // ── Modifier mask helper ─────────────────────────────────

    #[test]
    fn decode_modifier_mask_table() {
        // Per xterm spec: parameter is (mask + 1).
        assert_eq!(decode_modifier_mask(0), (false, false, false));
        // 1 → 0 → no mods
        assert_eq!(decode_modifier_mask(1), (false, false, false));
        // 2 → shift
        assert_eq!(decode_modifier_mask(2), (false, false, true));
        // 3 → alt
        assert_eq!(decode_modifier_mask(3), (false, true, false));
        // 4 → shift + alt
        assert_eq!(decode_modifier_mask(4), (false, true, true));
        // 5 → ctrl
        assert_eq!(decode_modifier_mask(5), (true, false, false));
        // 6 → shift + ctrl
        assert_eq!(decode_modifier_mask(6), (true, false, true));
        // 7 → alt + ctrl
        assert_eq!(decode_modifier_mask(7), (true, true, false));
        // 8 → shift + alt + ctrl
        assert_eq!(decode_modifier_mask(8), (true, true, true));
    }

    // ── Re-entry guard ───────────────────────────────────────

    #[test]
    fn inflight_guard_serialises_calls() {
        // First entry succeeds.
        let g1 = InflightGuard::try_enter().expect("first entry must succeed");
        // Re-entry while g1 is alive must fail.
        assert!(
            InflightGuard::try_enter().is_none(),
            "re-entry must be rejected"
        );
        // Drop g1 → next entry succeeds again.
        drop(g1);
        let g2 = InflightGuard::try_enter().expect("re-entry after drop must succeed");
        drop(g2);
    }

    // ── RawModeGuard restore (real fd, no real raw mode) ─────
    //
    // We can't enter real raw mode in `cargo test` because stdin is
    // typically not a TTY, but we *can* exercise the RAII shape on a
    // pty pair to verify that constructing and dropping a guard
    // restores the original termios.
    //
    // This is the core regression for RC2B-202.

    fn open_pty_pair() -> Option<(i32, i32)> {
        // openpty(3) → returns (master, slave). We use posix_openpt +
        // grantpt + unlockpt + ptsname to avoid the openpty linkage
        // (which lives in libutil on some glibc setups).
        let master = unsafe { libc::posix_openpt(libc::O_RDWR | libc::O_NOCTTY) };
        if master < 0 {
            return None;
        }
        if unsafe { libc::grantpt(master) } != 0 {
            unsafe { libc::close(master) };
            return None;
        }
        if unsafe { libc::unlockpt(master) } != 0 {
            unsafe { libc::close(master) };
            return None;
        }
        let name_ptr = unsafe { libc::ptsname(master) };
        if name_ptr.is_null() {
            unsafe { libc::close(master) };
            return None;
        }
        let slave = unsafe { libc::open(name_ptr, libc::O_RDWR | libc::O_NOCTTY) };
        if slave < 0 {
            unsafe { libc::close(master) };
            return None;
        }
        Some((master, slave))
    }

    fn termios_eq(a: &libc::termios, b: &libc::termios) -> bool {
        a.c_iflag == b.c_iflag
            && a.c_oflag == b.c_oflag
            && a.c_cflag == b.c_cflag
            && a.c_lflag == b.c_lflag
            && a.c_cc == b.c_cc
    }

    #[test]
    fn raw_mode_guard_restores_termios_on_normal_drop() {
        let (master, slave) = match open_pty_pair() {
            Some(p) => p,
            None => {
                eprintln!("skipping raw_mode_guard test: no pty available");
                return;
            }
        };

        // Snapshot original termios on the slave (the pty side that
        // looks like a terminal to the program).
        let mut before = MaybeUninit::<libc::termios>::zeroed();
        let rc = unsafe { libc::tcgetattr(slave, before.as_mut_ptr()) };
        assert_eq!(rc, 0, "tcgetattr on slave must succeed");
        let before = unsafe { before.assume_init() };

        // Enter raw mode → drop → snapshot again. The post-drop
        // termios must equal the pre-enter termios bit-for-bit.
        {
            let _g = RawModeGuard::enter(slave).expect("enter raw mode on slave pty");
            // While the guard is alive, the termios is *not* equal
            // to `before` (raw mode flipped lflag bits).
            let mut during = MaybeUninit::<libc::termios>::zeroed();
            unsafe { libc::tcgetattr(slave, during.as_mut_ptr()) };
            let during = unsafe { during.assume_init() };
            assert!(
                !termios_eq(&before, &during),
                "raw mode must change termios while guard is alive"
            );
        }
        // Guard dropped → restore should have run.
        let mut after = MaybeUninit::<libc::termios>::zeroed();
        let rc = unsafe { libc::tcgetattr(slave, after.as_mut_ptr()) };
        assert_eq!(rc, 0);
        let after = unsafe { after.assume_init() };
        assert!(
            termios_eq(&before, &after),
            "RawModeGuard::drop must restore the original termios"
        );

        unsafe {
            libc::close(slave);
            libc::close(master);
        }
    }

    #[test]
    fn raw_mode_guard_restores_termios_on_panic_unwind() {
        let (master, slave) = match open_pty_pair() {
            Some(p) => p,
            None => {
                eprintln!("skipping panic restore test: no pty available");
                return;
            }
        };
        let mut before = MaybeUninit::<libc::termios>::zeroed();
        unsafe { libc::tcgetattr(slave, before.as_mut_ptr()) };
        let before = unsafe { before.assume_init() };

        let result = std::panic::catch_unwind(|| {
            let _g = RawModeGuard::enter(slave).expect("enter raw mode");
            panic!("simulated panic inside raw mode");
        });
        assert!(result.is_err(), "the inner closure must propagate panic");

        // Even though the closure panicked, Drop ran and restored.
        let mut after = MaybeUninit::<libc::termios>::zeroed();
        unsafe { libc::tcgetattr(slave, after.as_mut_ptr()) };
        let after = unsafe { after.assume_init() };
        assert!(
            termios_eq(&before, &after),
            "RawModeGuard::drop must restore termios even on panic unwind"
        );

        unsafe {
            libc::close(slave);
            libc::close(master);
        }
    }
}

#[cfg(test)]
mod framing_tests {
    use super::*;

    fn pipe_with(payload: &[u8]) -> (i32, i32) {
        let mut fds = [0i32; 2];
        assert_eq!(unsafe { libc::pipe(fds.as_mut_ptr()) }, 0, "pipe(2)");
        let written = unsafe { libc::write(fds[1], payload.as_ptr() as *const _, payload.len()) };
        assert_eq!(written, payload.len() as isize, "write payload");
        (fds[0], fds[1])
    }

    fn expect_decoded(outcome: ReadOutcome) -> DecodedKey {
        match outcome {
            ReadOutcome::Decoded(k) => k,
            ReadOutcome::Eof => panic!("unexpected Eof"),
            ReadOutcome::Interrupted => panic!("unexpected Interrupted"),
            ReadOutcome::Io(e) => panic!("unexpected Io({e})"),
        }
    }

    /// Key auto-repeat regression: two complete arrow sequences are
    /// already buffered when the first read starts. The old greedy
    /// drain (stop only on poll timeout) concatenated both into one
    /// buffer, decoded only the first, and silently dropped the
    /// second. Sequence-structure framing must deliver both.
    #[test]
    fn read_one_key_delivers_repeated_sequences_one_by_one() {
        let (rfd, wfd) = pipe_with(b"\x1b[A\x1b[B");
        let first = expect_decoded(read_one_key_native(rfd));
        assert_eq!(first.kind, KeyKind::ArrowUp);
        let second = expect_decoded(read_one_key_native(rfd));
        assert_eq!(second.kind, KeyKind::ArrowDown);
        unsafe {
            libc::close(rfd);
            libc::close(wfd);
        }
    }

    /// A modifier CSI form must still be assembled completely, and a
    /// trailing plain byte must surface as its own event instead of
    /// being swallowed with the sequence.
    #[test]
    fn read_one_key_stops_at_csi_final_byte_before_following_input() {
        let (rfd, wfd) = pipe_with(b"\x1b[1;5Ax");
        let first = expect_decoded(read_one_key_native(rfd));
        assert_eq!(first.kind, KeyKind::ArrowUp);
        assert!(first.ctrl, "xterm modifier 5 = ctrl");
        let second = expect_decoded(read_one_key_native(rfd));
        assert_eq!(second.kind, KeyKind::Char);
        assert_eq!(second.text, "x");
        unsafe {
            libc::close(rfd);
            libc::close(wfd);
        }
    }

    /// SS3 function keys are exactly three bytes; the byte after them
    /// belongs to the next event.
    #[test]
    fn read_one_key_stops_after_ss3_sequence() {
        let (rfd, wfd) = pipe_with(b"\x1bOPq");
        let first = expect_decoded(read_one_key_native(rfd));
        assert_eq!(first.kind, KeyKind::F1);
        let second = expect_decoded(read_one_key_native(rfd));
        assert_eq!(second.kind, KeyKind::Char);
        assert_eq!(second.text, "q");
        unsafe {
            libc::close(rfd);
            libc::close(wfd);
        }
    }

    /// 16-byte-cap regression: five-plus repeats used to cut the 6th
    /// sequence mid-way, leaving `[`/`A` bytes to surface as garbage
    /// Char events. With framing, every sequence decodes cleanly.
    #[test]
    fn read_one_key_survives_a_long_repeat_burst() {
        let (rfd, wfd) = pipe_with(b"\x1b[A\x1b[A\x1b[A\x1b[A\x1b[A\x1b[A");
        for i in 0..6 {
            let k = expect_decoded(read_one_key_native(rfd));
            assert_eq!(k.kind, KeyKind::ArrowUp, "repeat #{i} must decode");
        }
        unsafe {
            libc::close(rfd);
            libc::close(wfd);
        }
    }

    /// Alt + multi-byte UTF-8 stays one (Unknown) event; the
    /// continuation bytes must not leak into the next read.
    #[test]
    fn read_one_key_keeps_alt_utf8_in_one_event() {
        let (rfd, wfd) = pipe_with("\u{1b}éz".as_bytes());
        let first = expect_decoded(read_one_key_native(rfd));
        assert_eq!(first.kind, KeyKind::Unknown);
        let second = expect_decoded(read_one_key_native(rfd));
        assert_eq!(second.kind, KeyKind::Char);
        assert_eq!(second.text, "z");
        unsafe {
            libc::close(rfd);
            libc::close(wfd);
        }
    }

    // ── Ctrl chords outside the letter range ──────────────────────

    #[test]
    fn decode_nul_is_ctrl_space() {
        let k = decode(&[0x00]);
        assert_eq!(k.kind, KeyKind::Char);
        assert_eq!(k.text, " ");
        assert!(k.ctrl);
        assert!(!k.alt);
    }

    #[test]
    fn decode_c0_punctuation_bytes_are_ctrl_chords() {
        for (byte, ch) in [(0x1Cu8, "\\"), (0x1D, "]"), (0x1E, "^"), (0x1F, "_")] {
            let k = decode(&[byte]);
            assert_eq!(k.kind, KeyKind::Char, "byte {byte:#04x}");
            assert_eq!(k.text, ch, "byte {byte:#04x}");
            assert!(k.ctrl, "byte {byte:#04x}");
        }
    }

    // ── Alt-prefixed (ESC ESC) sequences ──────────────────────────

    #[test]
    fn decode_esc_esc_csi_is_alt_arrow() {
        let k = decode(b"\x1b\x1b[A");
        assert_eq!(k.kind, KeyKind::ArrowUp);
        assert!(k.alt);
        assert!(!k.ctrl);
    }

    #[test]
    fn decode_esc_esc_alone_is_alt_escape() {
        let k = decode(&[0x1B, 0x1B]);
        assert_eq!(k.kind, KeyKind::Escape);
        assert!(k.alt);
    }

    /// Alt+Arrow arrives as `ESC ESC [ A`; the whole prefixed
    /// sequence must frame as one event (it used to decode as Unknown
    /// with the tail surfacing as `Char('[')` / `Char('A')`).
    #[test]
    fn read_one_key_assembles_alt_prefixed_arrow() {
        let (rfd, wfd) = pipe_with(b"\x1b\x1b[Ax");
        let first = expect_decoded(read_one_key_native(rfd));
        assert_eq!(first.kind, KeyKind::ArrowUp);
        assert!(first.alt);
        let second = expect_decoded(read_one_key_native(rfd));
        assert_eq!(second.kind, KeyKind::Char);
        assert_eq!(second.text, "x");
        unsafe {
            libc::close(rfd);
            libc::close(wfd);
        }
    }

    // ── X10 legacy mouse + oversized CSI framing ──────────────────

    /// A bare `ESC [ M` final carries three payload bytes; they must
    /// be consumed with the frame (one Unknown event), not surface as
    /// garbage Char events before the next real key.
    #[test]
    fn read_one_key_consumes_x10_mouse_payload() {
        let (rfd, wfd) = pipe_with(b"\x1b[M !!q");
        let first = expect_decoded(read_one_key_native(rfd));
        assert_eq!(first.kind, KeyKind::Unknown);
        let second = expect_decoded(read_one_key_native(rfd));
        assert_eq!(second.kind, KeyKind::Char);
        assert_eq!(second.text, "q");
        unsafe {
            libc::close(rfd);
            libc::close(wfd);
        }
    }

    /// A CSI longer than the 16-byte frame is drained to its final
    /// byte: the truncated frame decodes as one Unknown and the next
    /// read starts at the next real event instead of the sequence
    /// tail.
    #[test]
    fn read_one_key_drains_oversized_csi_to_its_final_byte() {
        let (rfd, wfd) = pipe_with(b"\x1b[1;2;3;4;5;6;7;8;9Az");
        let first = expect_decoded(read_one_key_native(rfd));
        assert_eq!(first.kind, KeyKind::Unknown);
        let second = expect_decoded(read_one_key_native(rfd));
        assert_eq!(second.kind, KeyKind::Char);
        assert_eq!(second.text, "z");
        unsafe {
            libc::close(rfd);
            libc::close(wfd);
        }
    }
}
