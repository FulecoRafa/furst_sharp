# FurstSharp justfile
# Run `just --list` to see all available recipes.

# Default: show available recipes
default:
    @just --list

# Build the full Rust workspace (produces librust_lib.so in target/debug/)
build-rust:
    cargo build

# Build the F# console application
build-fsharp:
    dotnet build example/fsharp-app/FsharpApp.fsproj

# Generate F# bindings from #[furst_export] annotations.
# TODO: implement actual codegen. For now this is a no-op placeholder.
codegen:
    @echo "codegen: not yet implemented."
    @echo "  Future: scan Rust source for #[furst_export], emit Generated/FurstBindings.fs"
    @echo "  The generated file is currently hand-written as a skeleton."

# Run the full pipeline: codegen -> build-rust -> build-fsharp -> run
run: codegen build-rust build-fsharp
    dotnet run --project example/fsharp-app/FsharpApp.fsproj

# Run all Rust tests
test:
    cargo test

# Fast type-check without linking (much faster than build)
check:
    cargo check

# Clean all build artifacts
clean:
    cargo clean
    dotnet clean example/fsharp-app/FsharpApp.fsproj

# Watch Rust sources and recheck on change (requires cargo-watch)
watch:
    cargo watch -x check
