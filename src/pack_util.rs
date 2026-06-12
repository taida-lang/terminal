//! Shared pack-construction tail.
//!
//! Builds the return pack and, when construction fails, releases the
//! child values — per the addon ABI contract their ownership stays
//! with the caller until the pack accepts them, so returning the null
//! without the rollback leaked every child on the host side. Mirrors
//! the windows-side `build_key_pack` rollback so every entry point's
//! pack construction is leak-symmetric.

use core::ffi::c_char;

use taida_addon::TaidaAddonValueV1;
use taida_addon::bridge::HostValueBuilder;

pub(crate) fn pack_or_release(
    builder: &HostValueBuilder<'_>,
    names: &[*const c_char],
    values: &[*mut TaidaAddonValueV1],
) -> *mut TaidaAddonValueV1 {
    let pack = builder.pack(names, values);
    if pack.is_null() {
        for &v in values {
            if !v.is_null() {
                unsafe { builder.release(v) };
            }
        }
    }
    pack
}
