# FurstSharp justfile
# Run `just --list` to see all available recipes.

# Default: show available recipes
default:
    @just --list

# Fast type-check without linking (much faster than build)
check:
    cargo check

# Build the full Rust workspace (produces librust_lib.so in target/debug/)
build-rust:
    cargo build

# Build Rust in release mode
build-release:
    cargo build --release

# Build only the furst-codegen binary
build-codegen:
    cargo build -p furst-codegen

# Generate F# bindings from #[furst_export] annotations
codegen: build-codegen
    cargo run -p furst-codegen -- \
        --input example/rust-lib/src \
        --output example/fsharp-app/Generated/FurstBindings.fs \
        --lib-name rust_lib

# Build the F# console application
build-fsharp:
    dotnet build example/fsharp-app/FsharpApp.fsproj

# Run the full pipeline: codegen -> build-rust -> build-fsharp -> dotnet run
run: codegen build-rust build-fsharp
    LD_LIBRARY_PATH=target/debug dotnet run --project example/fsharp-app/FsharpApp.fsproj

# Run all Rust tests
test:
    cargo test

# Build a release bundle: compiled .so + FurstBindings.fs in dist/
bundle: build-release codegen
    mkdir -p dist
    cargo run -p furst-codegen -- \
        --input example/rust-lib/src \
        --output dist/FurstBindings.fs \
        --lib-name rust_lib \
        --bundle-dir dist \
        --lib-path target/release/librust_lib.so

# Clean all build artifacts
clean:
    cargo clean
    dotnet clean example/fsharp-app/FsharpApp.fsproj
    rm -rf dist

# Expand #[furst_export] macros in example/rust-lib to see generated code (requires nightly)
expand:
    cargo +nightly rustc --manifest-path example/rust-lib/Cargo.toml -- -Z unpretty=expanded

# Watch Rust sources and recheck on change (requires cargo-watch)
watch:
    cargo watch -x check
