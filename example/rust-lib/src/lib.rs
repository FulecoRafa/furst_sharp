//! Example shared library demonstrating `#[furst_export]`.
//!
//! Build with: `cargo build -p rust-lib`
//! Output: `target/debug/librust_lib.so` (Linux)
//!
//! Covers every supported export kind:
//!   - Primitives:      fibonacci
//!   - Struct:          Point + distance
//!   - C-style enum:    Direction + turn_right
//!   - Tagged enum:     Shape (Circle/Rectangle) + area
//!   - Strings:         greet (&str → String)
//!   - Opaque handles:  Counter (heap-allocated, *mut T)

use furst_macro::furst_export;

// ─── Primitives ───────────────────────────────────────────────────────────

/// Compute the nth Fibonacci number (iterative, no stack overflow).
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

// ─── Struct ───────────────────────────────────────────────────────────────

/// A 2-D point. Exported as a `#[repr(C)]` struct; passed by value over FFI.
#[furst_export]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

/// Euclidean distance between two points.
#[furst_export]
pub fn distance(a: Point, b: Point) -> f64 {
    let dx = a.x - b.x;
    let dy = a.y - b.y;
    (dx * dx + dy * dy).sqrt()
}

// ─── C-style enum ─────────────────────────────────────────────────────────

/// Cardinal compass direction. No associated data → C-style enum (`#[repr(i32)]`).
#[furst_export]
pub enum Direction {
    North,
    East,
    South,
    West,
}

/// Turn 90° clockwise.
#[furst_export]
pub fn turn_right(d: Direction) -> Direction {
    match d {
        Direction::North => Direction::East,
        Direction::East => Direction::South,
        Direction::South => Direction::West,
        Direction::West => Direction::North,
    }
}

// ─── Tagged enum ──────────────────────────────────────────────────────────

/// A geometric shape. Tagged union: the macro generates `ShapeTag`,
/// `ShapeCircleData`, `ShapeRectangleData`, `ShapeUnion`, `ShapeFfi`,
/// and a `From<Shape> for ShapeFfi` impl.
#[furst_export]
pub enum Shape {
    Circle { radius: f64 },
    Rectangle { width: f64, height: f64 },
}

/// Area of a shape, accepting the FFI wrapper type `ShapeFfi`.
/// Union fields must be accessed via `unsafe` because Rust cannot statically
/// know which variant is active — that information lives in `shape.tag`.
#[furst_export]
pub fn area(shape: ShapeFfi) -> f64 {
    match shape.tag {
        ShapeTag::Circle => {
            let r = unsafe { shape.data.circle.radius };
            std::f64::consts::PI * r * r
        }
        ShapeTag::Rectangle => unsafe { shape.data.rectangle.width * shape.data.rectangle.height },
    }
}

// ─── Strings ──────────────────────────────────────────────────────────────

/// Return a greeting string. Demonstrates `&str` input (split to ptr+len by
/// the macro) and `String` return (wrapped in `FurstStr`; caller must free).
#[furst_export]
pub fn greet(name: &str) -> String {
    format!("Hello, {}!", name)
}

// ─── Opaque handles (impl block pattern) ─────────────────────────────────

/// A heap-allocated counter. Demonstrates the `#[furst_export] impl` pattern:
/// write idiomatic Rust methods, get FFI functions + typed F# handles for free.
pub struct Counter {
    value: i64,
}

/// The macro generates: `counter_new`, `counter_increment`, `counter_get`,
/// and auto-generates `counter_free` (since we didn't define one).
#[furst_export]
impl Counter {
    pub fn new(initial: i64) -> Self {
        Counter { value: initial }
    }

    pub fn increment(&mut self) {
        self.value += 1;
    }

    pub fn get(&self) -> i64 {
        self.value
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fibonacci_base_cases() {
        assert_eq!(fibonacci(0), 0);
        assert_eq!(fibonacci(1), 1);
    }

    #[test]
    fn fibonacci_known_values() {
        assert_eq!(fibonacci(5), 5);
        assert_eq!(fibonacci(10), 55);
        assert_eq!(fibonacci(20), 6765);
    }

    #[test]
    fn distance_3_4_triangle() {
        let a = Point { x: 0.0, y: 0.0 };
        let b = Point { x: 3.0, y: 4.0 };
        assert!((distance(a, b) - 5.0).abs() < 1e-10);
    }

    #[test]
    fn turn_right_cycle() {
        assert_eq!(turn_right(Direction::North) as i32, Direction::East as i32);
        assert_eq!(turn_right(Direction::East) as i32, Direction::South as i32);
        assert_eq!(turn_right(Direction::South) as i32, Direction::West as i32);
        assert_eq!(turn_right(Direction::West) as i32, Direction::North as i32);
    }

    #[test]
    fn area_circle() {
        let s = Shape::Circle { radius: 1.0 };
        let ffi = ShapeFfi::from(s);
        let a = area(ffi);
        assert!((a - std::f64::consts::PI).abs() < 1e-10);
    }

    #[test]
    fn area_rectangle() {
        let s = Shape::Rectangle {
            width: 3.0,
            height: 4.0,
        };
        let ffi = ShapeFfi::from(s);
        assert_eq!(area(ffi), 12.0);
    }

    #[test]
    fn greet_returns_correct_string() {
        // Test via the inner function (no FFI boundary in tests)
        let result = __furst_inner_greet("World");
        assert_eq!(result, "Hello, World!");
    }

    #[test]
    fn counter_lifecycle() {
        let c = counter_new(10);
        assert_eq!(counter_get(c), 10);
        counter_increment(c);
        counter_increment(c);
        assert_eq!(counter_get(c), 12);
        counter_free(c);
    }

    #[test]
    fn counter_zero() {
        let c = counter_new(0);
        assert_eq!(counter_get(c), 0);
        counter_increment(c);
        assert_eq!(counter_get(c), 1);
        counter_free(c);
    }
}
