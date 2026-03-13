//! `furst-macro`: proc-macro crate for the FurstSharp project.
//!
//! Provides the `#[furst_export]` attribute macro, which will eventually:
//!   1. Generate an `extern "C" #[no_mangle]` wrapper with C-ABI compatible types.
//!   2. Emit F# P/Invoke binding metadata for the codegen step to consume.
//!
//! **Current status**: SKELETON — the macro is a pass-through that returns the
//! annotated item unchanged so that consuming crates compile as-is.

use proc_macro::TokenStream;

/// Mark a Rust function for export to F# via FFI (P/Invoke).
///
/// # Example
///
/// ```rust,ignore
/// use furst_macro::furst_export;
///
/// #[furst_export]
/// pub fn add(a: i64, b: i64) -> i64 {
///     a + b
/// }
/// ```
///
/// Once fully implemented, this macro will:
/// - Parse the function signature with `syn::ItemFn`
/// - Validate that all parameter/return types are FFI-safe
/// - Generate `#[no_mangle] pub extern "C" fn __furst_<name>(...)` wrapper
/// - Emit metadata for the codegen step to produce `FurstBindings.fs`
///
/// # TODO
/// - [ ] Parse `syn::ItemFn` and validate FFI-safe types
/// - [ ] Generate `extern "C"` wrapper via `quote!`
/// - [ ] Design and implement the F# codegen side-channel mechanism
#[proc_macro_attribute]
pub fn furst_export(_attr: TokenStream, item: TokenStream) -> TokenStream {
    // SKELETON: return the item completely unchanged.
    // Real implementation will parse `item`, append the C wrapper, and emit
    // codegen metadata.
    item
}
