//! Example shared library demonstrating `#[furst_export]`.
//!
//! Build with: `cargo build -p rust-lib`
//! Output: `target/debug/librust_lib.so` (Linux)
//!
//! NOTE: The `#[no_mangle] pub extern "C"` annotations are written by hand
//! because the macro is currently a pass-through skeleton. Once the macro is
//! fully implemented, it will generate the `extern "C"` wrapper automatically
//! and these manual annotations can be removed from user code.

use furst_macro::furst_export;

/// Compute the nth Fibonacci number.
///
/// This function is exported via FFI for F# consumption. The iterative
/// implementation avoids stack overflow on large values of `n`.
#[furst_export]
#[no_mangle]
pub extern "C" fn fibonacci(n: i64) -> i64 {
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
