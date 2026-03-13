//! `furst-codegen`: scans Rust source files for `#[furst_export]` items and
//! emits a `FurstBindings.fs` F# module with P/Invoke declarations.
//!
//! Usage:
//!   furst-codegen --input <dir/file>... --output <path> [--lib-name <name>]
//!                 [--bundle-dir <dir> --lib-path <so>]

use std::{
    collections::HashSet,
    fmt::Write as FmtWrite,
    fs,
    path::PathBuf,
};

use clap::Parser;
use syn::visit::Visit;
use walkdir::WalkDir;

// ─── CLI ──────────────────────────────────────────────────────────────────

#[derive(Parser, Debug)]
#[command(about = "Generate F# P/Invoke bindings from #[furst_export] annotations")]
struct Cli {
    /// Rust source files or directories to scan
    #[arg(long, required = true, num_args = 1..)]
    input: Vec<PathBuf>,

    /// Output path for FurstBindings.fs
    #[arg(long)]
    output: PathBuf,

    /// Native library name for DllImport (no lib prefix or extension)
    #[arg(long, default_value = "rust_lib")]
    lib_name: String,

    /// Copy the generated .fs and .so here to form a release bundle
    #[arg(long)]
    bundle_dir: Option<PathBuf>,

    /// Path to the compiled .so (required when --bundle-dir is set)
    #[arg(long)]
    lib_path: Option<PathBuf>,
}

// ─── FFI type model ───────────────────────────────────────────────────────

#[derive(Clone, Debug)]
enum FfiType {
    I32,
    I64,
    U32,
    U64,
    F32,
    F64,
    Bool,
    Unit,
    StrRef,
    OwnedString,
    Named(String),
}

fn classify(ty: &syn::Type) -> Option<FfiType> {
    match ty {
        syn::Type::Path(tp) if tp.qself.is_none() => {
            let name = tp.path.segments.last()?.ident.to_string();
            Some(match name.as_str() {
                "i32" => FfiType::I32,
                "i64" => FfiType::I64,
                "u32" => FfiType::U32,
                "u64" => FfiType::U64,
                "f32" => FfiType::F32,
                "f64" => FfiType::F64,
                "bool" => FfiType::Bool,
                "String" => FfiType::OwnedString,
                other => FfiType::Named(other.to_string()),
            })
        }
        syn::Type::Reference(r) => {
            if let syn::Type::Path(tp) = r.elem.as_ref() {
                if tp.path.is_ident("str") {
                    return Some(FfiType::StrRef);
                }
            }
            None
        }
        syn::Type::Tuple(t) if t.elems.is_empty() => Some(FfiType::Unit),
        _ => None,
    }
}

/// Map FfiType → F# P/Invoke type string (single value; &str is handled separately)
fn fsharp_type(ty: &FfiType, tagged_enum_names: &HashSet<String>) -> String {
    match ty {
        FfiType::I32 => "int32".into(),
        FfiType::I64 => "int64".into(),
        FfiType::U32 => "uint32".into(),
        FfiType::U64 => "uint64".into(),
        FfiType::F32 => "float32".into(),
        FfiType::F64 => "float".into(),
        FfiType::Bool => "bool".into(),
        FfiType::Unit => "void".into(),
        FfiType::StrRef => "nativeint".into(), // placeholder; exploded in param lists
        FfiType::OwnedString => "FurstStr".into(),
        FfiType::Named(n) => {
            if tagged_enum_names.contains(n.as_str()) {
                format!("{}Ffi", n)
            } else {
                n.clone()
            }
        }
    }
}

// ─── Collected export items ───────────────────────────────────────────────

enum ExportItem {
    Fn(syn::ItemFn),
    Struct(syn::ItemStruct),
    Enum(syn::ItemEnum),
}

// ─── Visitor ──────────────────────────────────────────────────────────────

#[derive(Default)]
struct FurstVisitor {
    exports: Vec<ExportItem>,
}

fn has_furst_export(attrs: &[syn::Attribute]) -> bool {
    attrs
        .iter()
        .any(|a| a.path().is_ident("furst_export"))
}

impl<'ast> Visit<'ast> for FurstVisitor {
    fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
        if has_furst_export(&node.attrs) {
            self.exports.push(ExportItem::Fn(node.clone()));
        }
    }

    fn visit_item_struct(&mut self, node: &'ast syn::ItemStruct) {
        if has_furst_export(&node.attrs) {
            self.exports.push(ExportItem::Struct(node.clone()));
        }
    }

    fn visit_item_enum(&mut self, node: &'ast syn::ItemEnum) {
        if has_furst_export(&node.attrs) {
            self.exports.push(ExportItem::Enum(node.clone()));
        }
    }
}

// ─── Scanning ─────────────────────────────────────────────────────────────

fn collect_exports(inputs: &[PathBuf]) -> Vec<ExportItem> {
    let mut visitor = FurstVisitor::default();

    for input in inputs {
        let paths: Box<dyn Iterator<Item = PathBuf>> = if input.is_dir() {
            Box::new(
                WalkDir::new(input)
                    .into_iter()
                    .filter_map(|e| e.ok())
                    .filter(|e| e.path().extension().map_or(false, |x| x == "rs"))
                    .map(|e| e.into_path()),
            )
        } else {
            Box::new(std::iter::once(input.clone()))
        };

        for path in paths {
            let src = match fs::read_to_string(&path) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("warning: could not read {}: {}", path.display(), e);
                    continue;
                }
            };
            let file = match syn::parse_file(&src) {
                Ok(f) => f,
                Err(e) => {
                    eprintln!("warning: could not parse {}: {}", path.display(), e);
                    continue;
                }
            };
            visitor.visit_file(&file);
        }
    }

    visitor.exports
}

// ─── F# generation ────────────────────────────────────────────────────────

fn generate_fsharp(exports: &[ExportItem], lib_name: &str) -> String {
    // First pass: catalog tagged enum names so Named(...) can map to <N>Ffi
    let tagged_enum_names: HashSet<String> = exports
        .iter()
        .filter_map(|e| {
            if let ExportItem::Enum(enm) = e {
                let is_tagged = enm.variants.iter().any(|v| !v.fields.is_empty());
                if is_tagged {
                    return Some(enm.ident.to_string());
                }
            }
            None
        })
        .collect();

    let mut out = String::new();

    // File header
    writeln!(out, "/// FurstBindings.fs").unwrap();
    writeln!(out, "///").unwrap();
    writeln!(out, "/// AUTO-GENERATED by furst-codegen — do not edit by hand.").unwrap();
    writeln!(out, "module FurstBindings").unwrap();
    writeln!(out).unwrap();
    writeln!(out, "open System.Runtime.InteropServices").unwrap();
    writeln!(out).unwrap();

    // Native lib constant
    writeln!(out, "[<Literal>]").unwrap();
    writeln!(out, "let private NativeLib = \"lib{}\"", lib_name).unwrap();
    writeln!(out).unwrap();

    // Always emit FurstStr and furst_free_string
    writeln!(out, "// --- Runtime types (managed by Rust) ---").unwrap();
    writeln!(out).unwrap();
    writeln!(out, "[<Struct; StructLayout(LayoutKind.Sequential)>]").unwrap();
    writeln!(out, "type FurstStr =").unwrap();
    writeln!(out, "    val mutable ptr: nativeint").unwrap();
    writeln!(out, "    val mutable len: unativeint").unwrap();
    writeln!(out, "    val mutable cap: unativeint").unwrap();
    writeln!(out).unwrap();
    writeln!(out, "[<DllImport(NativeLib, EntryPoint = \"furst_free_string\", CallingConvention = CallingConvention.Cdecl)>]").unwrap();
    writeln!(out, "extern void furst_free_string(FurstStr s)").unwrap();
    writeln!(out).unwrap();

    // Second pass: emit each item
    writeln!(out, "// --- Exported bindings ---").unwrap();

    for item in exports {
        writeln!(out).unwrap();
        match item {
            ExportItem::Fn(func) => emit_fn(&mut out, func, &tagged_enum_names),
            ExportItem::Struct(strct) => emit_struct(&mut out, strct, &tagged_enum_names),
            ExportItem::Enum(enm) => emit_enum(&mut out, enm, &tagged_enum_names),
        }
    }

    out
}

fn emit_fn(out: &mut String, func: &syn::ItemFn, tagged: &HashSet<String>) {
    let name = func.sig.ident.to_string();

    // Classify return type
    let ret_ffi = match &func.sig.output {
        syn::ReturnType::Default => FfiType::Unit,
        syn::ReturnType::Type(_, ty) => match classify(ty) {
            Some(t) => t,
            None => {
                eprintln!("warning: skipping fn `{}` — unsupported return type", name);
                return;
            }
        },
    };

    // Build parameter list, exploding &str into ptr+len pairs
    let mut params: Vec<String> = Vec::new();
    for input in &func.sig.inputs {
        let syn::FnArg::Typed(pt) = input else {
            continue;
        };
        let pname = match pt.pat.as_ref() {
            syn::Pat::Ident(pi) => pi.ident.to_string(),
            _ => {
                eprintln!("warning: skipping fn `{}` — complex parameter pattern", name);
                return;
            }
        };
        let ffi = match classify(&pt.ty) {
            Some(t) => t,
            None => {
                eprintln!(
                    "warning: skipping fn `{}` — unsupported parameter type for `{}`",
                    name, pname
                );
                return;
            }
        };
        match ffi {
            FfiType::StrRef => {
                params.push(format!("nativeint {}_ptr", pname));
                params.push(format!("unativeint {}_len", pname));
            }
            FfiType::OwnedString => {
                eprintln!(
                    "warning: skipping fn `{}` — `String` is not supported as a parameter type",
                    name
                );
                return;
            }
            other => {
                params.push(format!("{} {}", fsharp_type(&other, tagged), pname));
            }
        }
    }

    let ret_str = match &ret_ffi {
        FfiType::Unit => "void".to_string(),
        other => fsharp_type(other, tagged),
    };
    let params_str = params.join(", ");

    writeln!(
        out,
        "[<DllImport(NativeLib, EntryPoint = \"{name}\", CallingConvention = CallingConvention.Cdecl)>]"
    )
    .unwrap();
    writeln!(out, "extern {ret_str} {name}({params_str})").unwrap();
}

fn emit_struct(out: &mut String, strct: &syn::ItemStruct, tagged: &HashSet<String>) {
    let name = strct.ident.to_string();

    let syn::Fields::Named(named) = &strct.fields else {
        eprintln!(
            "warning: skipping struct `{}` — only named fields are supported",
            name
        );
        return;
    };

    writeln!(out, "[<Struct; StructLayout(LayoutKind.Sequential)>]").unwrap();
    writeln!(out, "type {} =", name).unwrap();
    for field in &named.named {
        let fname = field.ident.as_ref().unwrap().to_string();
        let ffi = match classify(&field.ty) {
            Some(t) => t,
            None => {
                eprintln!(
                    "warning: skipping field `{}::{}` — unsupported type",
                    name, fname
                );
                continue;
            }
        };
        writeln!(out, "    val mutable {}: {}", fname, fsharp_type(&ffi, tagged)).unwrap();
    }
}

fn emit_enum(out: &mut String, enm: &syn::ItemEnum, tagged: &HashSet<String>) {
    let name = enm.ident.to_string();
    let is_tagged = enm.variants.iter().any(|v| !v.fields.is_empty());

    if !is_tagged {
        // C-style enum
        writeln!(out, "type {} =", name).unwrap();
        for (i, variant) in enm.variants.iter().enumerate() {
            writeln!(out, "    | {} = {}", variant.ident, i).unwrap();
        }
        return;
    }

    // Tagged union — emit Tag enum + Data structs + Union + Ffi wrapper
    let tag_name = format!("{}Tag", name);
    let union_name = format!("{}Union", name);
    let ffi_name = format!("{}Ffi", name);

    // Tag enum
    writeln!(out, "type {} =", tag_name).unwrap();
    for (i, variant) in enm.variants.iter().enumerate() {
        writeln!(out, "    | {} = {}", variant.ident, i).unwrap();
    }
    writeln!(out).unwrap();

    // Variant data structs
    let mut variant_data_names: Vec<(String, String)> = Vec::new(); // (snake_name, data_type_name)
    for variant in &enm.variants {
        let vname = variant.ident.to_string();
        let data_name = format!("{}{}Data", name, vname);
        let snake = to_snake(&vname);
        variant_data_names.push((snake, data_name.clone()));

        writeln!(out, "[<Struct; StructLayout(LayoutKind.Sequential)>]").unwrap();
        writeln!(out, "type {} =", data_name).unwrap();

        match &variant.fields {
            syn::Fields::Unit => {
                // No fields — emit a dummy byte to avoid zero-size struct issues
                writeln!(out, "    val mutable _pad: byte").unwrap();
            }
            syn::Fields::Named(named) => {
                for field in &named.named {
                    let fname = field.ident.as_ref().unwrap().to_string();
                    let ffi = match classify(&field.ty) {
                        Some(t) => t,
                        None => {
                            eprintln!(
                                "warning: unsupported type in enum `{}` variant `{}` field `{}`",
                                name, vname, fname
                            );
                            continue;
                        }
                    };
                    writeln!(out, "    val mutable {}: {}", fname, fsharp_type(&ffi, tagged))
                        .unwrap();
                }
            }
            syn::Fields::Unnamed(_) => {
                eprintln!(
                    "warning: skipping tuple variant `{}::{}` in tagged enum",
                    name, vname
                );
            }
        }
        writeln!(out).unwrap();
    }

    // Union
    writeln!(out, "[<Struct; StructLayout(LayoutKind.Explicit)>]").unwrap();
    writeln!(out, "type {} =", union_name).unwrap();
    for (snake, data_name) in &variant_data_names {
        writeln!(out, "    [<FieldOffset(0)>] val mutable {}: {}", snake, data_name).unwrap();
    }
    writeln!(out).unwrap();

    // Ffi wrapper struct
    writeln!(out, "[<Struct; StructLayout(LayoutKind.Sequential)>]").unwrap();
    writeln!(out, "type {} =", ffi_name).unwrap();
    writeln!(out, "    val mutable tag: {}", tag_name).unwrap();
    writeln!(out, "    val mutable data: {}", union_name).unwrap();
}

fn to_snake(s: &str) -> String {
    let mut out = String::new();
    for (i, ch) in s.char_indices() {
        if ch.is_uppercase() && i > 0 {
            out.push('_');
        }
        out.push(ch.to_lowercase().next().unwrap());
    }
    out
}

// ─── Main ─────────────────────────────────────────────────────────────────

fn main() {
    let cli = Cli::parse();

    // Collect all #[furst_export] items from input paths
    let exports = collect_exports(&cli.input);

    if exports.is_empty() {
        eprintln!("warning: no #[furst_export] items found in the specified inputs");
    }

    // Generate the F# bindings file
    let fsharp = generate_fsharp(&exports, &cli.lib_name);

    // Write to --output
    if let Some(parent) = cli.output.parent() {
        fs::create_dir_all(parent).expect("failed to create output directory");
    }
    fs::write(&cli.output, &fsharp).expect("failed to write output file");
    println!("furst-codegen: wrote {}", cli.output.display());

    // Bundle: copy .fs and .so to --bundle-dir
    if let Some(bundle_dir) = &cli.bundle_dir {
        fs::create_dir_all(bundle_dir).expect("failed to create bundle directory");

        let fs_dest = bundle_dir.join(
            cli.output
                .file_name()
                .unwrap_or_else(|| std::ffi::OsStr::new("FurstBindings.fs")),
        );
        fs::write(&fs_dest, &fsharp).expect("failed to write .fs to bundle dir");
        println!("furst-codegen: bundle .fs → {}", fs_dest.display());

        if let Some(lib_path) = &cli.lib_path {
            let so_name = lib_path
                .file_name()
                .expect("--lib-path must point to a file");
            let so_dest = bundle_dir.join(so_name);
            fs::copy(lib_path, &so_dest).expect("failed to copy .so to bundle dir");
            println!("furst-codegen: bundle .so → {}", so_dest.display());
        }
    }
}
