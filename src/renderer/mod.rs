//! Renderer native implementation (Phase 8 / TMB-020).
//!
//! Layer A (`TM_DESIGN.md`): pure computation on `ScreenBuffer` /
//! `Cell` / `DiffOp` values. No syscalls, no signal handlers, no
//! termios. The Taida-side facade in `taida/renderer.td` becomes a
//! thin dispatch thunk over the entries here.
//!
//! Sub-modules:
//! - [`state`]: type definitions + pack <-> Vec marshalling.
//! - [`ops`]: `buffer_put` / `buffer_write` / `buffer_fill_rect` /
//!   `buffer_clear` mutating entries.
//! - [`diff`]: `buffer_diff` / `render_full` / `render_ops` /
//!   `render_frame` immutable computations.
//!
//! The split exists so the marshalling primitives can be reused by
//! the criterion benches without pulling in the FFI dispatcher
//! shape from `lib.rs`.

pub mod diff;
pub mod ops;
pub mod state;
