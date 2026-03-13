# FurstSharp

> Export Rust functions to F# with zero boilerplate via a proc-macro attribute.

## Overview

Annotate any Rust function with `#[furst_export]`:

```rust
use furst_macro::furst_export;

#[furst_export]
pub fn fibonacci(n: i64) -> i64 { /* ... */ }
```

FurstSharp generates the `extern "C"` Rust wrapper and the matching F#
`DllImport` binding, so you can call it from F# without writing any FFI glue:

```fsharp
open FurstBindings

printfn "%d" (fibonacci 10L)  // prints: 55
```

## Status

**Early skeleton.** The macro is currently a pass-through and F# bindings are
hand-written. See `CLAUDE.md` for implementation status and next steps.

## Prerequisites

| Tool   | Version   | Notes                        |
|--------|-----------|------------------------------|
| Rust   | stable    | via rustup                   |
| .NET   | 9.x       | `dotnet` CLI                 |
| just   | latest    | task runner                  |
| mise   | any       | optional, for pinned versions|

## Quick Start

```bash
# Install pinned tool versions (requires mise)
mise install

# Build Rust .so, build F# app, and run
just run
```

Expected output:
```
FurstSharp example — calling Rust fibonacci via P/Invoke

  fibonacci(0) = 0
  fibonacci(1) = 1
  fibonacci(5) = 5
  fibonacci(10) = 55
  fibonacci(20) = 6765

Success!
```

## Project Structure

```
furst-macro/              # The proc-macro crate (the library being built)
  src/lib.rs              #   #[furst_export] attribute macro
example/
  rust-lib/               # Example Rust cdylib using #[furst_export]
    src/lib.rs            #   fibonacci function
  fsharp-app/             # F# console app calling the Rust library
    Generated/
      FurstBindings.fs    #   P/Invoke declarations (auto-generated, skeleton for now)
    Program.fs            #   Entry point
```

## Available Tasks

| Command              | Description                                  |
|----------------------|----------------------------------------------|
| `just build-rust`    | `cargo build` → produces `librust_lib.so`    |
| `just build-fsharp`  | `dotnet build` the F# project                |
| `just codegen`       | Generate F# bindings (placeholder)           |
| `just run`           | Full pipeline: codegen + build + run         |
| `just test`          | `cargo test` for the Rust crates             |
| `just check`         | `cargo check` (fast type-check, no linking)  |
| `just clean`         | Remove all build artifacts                   |
| `just watch`         | Recheck on file change (requires cargo-watch)|

## How It Works

1. `cargo build` compiles `example/rust-lib` → `target/debug/librust_lib.so`
2. The `#[no_mangle] extern "C"` on `fibonacci` gives it a stable C symbol
3. F#'s `[<DllImport(...)>]` declares the native signature
4. `dotnet run` loads the `.so` at runtime and calls `fibonacci` via P/Invoke

The P/Invoke runtime on Linux resolves `libXXX.so` from the bare name `XXX`,
so no file extension is needed in the `DllImport` path.

## FFI Type Conventions

| Rust type | C ABI type  | F# P/Invoke type |
|-----------|-------------|------------------|
| `i64`     | `int64_t`   | `int64`          |
| `i32`     | `int32_t`   | `int32`          |
| `f64`     | `double`    | `float`          |
| `bool`    | `uint8_t`   | `bool`           |

All exported functions use `CallingConvention.Cdecl` (Rust `extern "C"` default).

## Development Notes

- The native library path in `FurstBindings.fs` is hardcoded as a relative
  path for development. Production packaging requires a different strategy
  (e.g., `NativeLibrary.SetDllImportResolver` or copying the `.so` alongside
  the F# binary).
- Only primitive types are supported in the skeleton. Structs, strings, and
  slices will require explicit marshalling design.
