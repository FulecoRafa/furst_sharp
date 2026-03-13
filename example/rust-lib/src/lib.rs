//! Example shared library demonstrating `#[furst_export]`.
//!
//! Build with: `cargo build -p rust-lib`
//! Output: `target/debug/librust_lib.so` (Linux)
//!
//! The macro generates all `#[no_mangle] pub extern "C"` boilerplate.

use furst_macro::furst_export;

/// Compute the nth Fibonacci number.
///
/// Exported via FFI for F# consumption. The iterative implementation
/// avoids stack overflow on large values of `n`.
#[furst_export]
pub fn fibonacci(n: i64) -> i64 {
    if n <= 1 {
        return n;
    }
    let (mut a, mut b) = (0i64, 1i64);
    for _ in 2..=n {
        let c = a.saturating_add(b);
        a = b;
        b = c;
    }
    b
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_cases() {
        assert_eq!(fibonacci(0), 0);
        assert_eq!(fibonacci(1), 1);
    }

    #[test]
    fn known_values() {
        assert_eq!(fibonacci(5), 5);
        assert_eq!(fibonacci(10), 55);
        assert_eq!(fibonacci(20), 6765);
    }
}
