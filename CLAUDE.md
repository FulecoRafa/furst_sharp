# FurstSharp — Claude Context

## What This Project Does
FurstSharp lets you annotate Rust functions with `#[furst_export]` and
automatically get F# P/Invoke bindings. Rust compiles to a `.so` shared
library; F# calls it via `DllImport`.

## Repository Layout
- `furst-macro/` — the proc-macro crate (the core library being built)
- `example/rust-lib/` — cdylib crate demonstrating `#[furst_export]` usage
- `example/fsharp-app/` — F# console app that calls the Rust library
- `example/fsharp-app/Generated/FurstBindings.fs` — placeholder for codegen output

## Build Commands
```
just check         # cargo check (fast)
just build-rust    # cargo build → produces target/debug/librust_lib.so
just build-fsharp  # dotnet build
just run           # full pipeline: codegen + build + dotnet run
just test          # cargo test
just clean         # remove all artifacts
```

## Current Status: Skeleton
- `#[furst_export]` macro is a **pass-through** — returns its input unchanged
- `example/rust-lib` writes `#[no_mangle] extern "C"` by hand (temporary)
- `Generated/FurstBindings.fs` is hand-written until codegen is implemented

## Key Design Decisions
- `furst-macro` lives at root (not under `example/`) — it's the product
- F# uses `CallingConvention.Cdecl` to match Rust `extern "C"`
- Native lib path in `FurstBindings.fs` is relative; works via `dotnet run`
- `just run` is the one-stop command for the full pipeline

## FFI Type Mapping
| Rust  | F# P/Invoke |
|-------|-------------|
| `i64` | `int64`     |
| `i32` | `int32`     |
| `f64` | `float`     |
| `bool`| `bool`      |

## Next Steps (Priority Order)
1. Implement the macro: parse `ItemFn`, validate FFI types, generate `extern "C"` wrapper
2. Design the codegen mechanism (build.rs manifest? separate binary?)
3. Implement `just codegen` to write `Generated/FurstBindings.fs`
4. Add type validation in macro for FFI-unsafe types
