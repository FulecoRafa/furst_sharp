//! `furst-macro`: proc-macro crate for the FurstSharp project.
//!
//! Provides the `#[furst_export]` attribute macro, which:
//!   1. Generates an `extern "C" #[no_mangle]` wrapper with C-ABI compatible types.
//!   2. Supports functions, structs, and enums (both C-style and tagged unions).
//!
//! # Type support
//! - Primitives: `i32`, `i64`, `u32`, `u64`, `f32`, `f64`, `bool`
//! - Strings: `&str` (input) splits into `(ptr: *const u8, len: usize)`
//! - Owned strings: `String` (return only) becomes `::furst_rt::FurstStr`
//! - Opaque handles: `*mut T` / `*const T` pass through as raw pointers (F# `nativeint`)
//! - Structs: gains `#[repr(C)]`, all fields must be FFI-safe
//! - C-style enums: gains `#[repr(i32)]`
//! - Tagged enums: generates Tag enum + Data structs + Union + Ffi wrapper

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::{ItemEnum, ItemFn, ItemImpl, ItemStruct};

// ─── Type classification ───────────────────────────────────────────────────

/// Internal representation of an FFI-compatible type.
#[derive(Clone)]
enum FfiType {
    I32,
    I64,
    U32,
    U64,
    F32,
    F64,
    Bool,
    Unit,
    /// `&str` input — expands to (ptr: *const u8, len: usize)
    StrRef,
    /// `String` return — becomes ::furst_rt::FurstStr
    OwnedString,
    /// A named type assumed to be a #[furst_export] struct or enum
    Named(syn::Ident),
    /// `*mut T` or `*const T` — opaque handle, passes through as a raw pointer.
    /// Maps to `nativeint` in F#.
    OpaquePtr(syn::Type),
}

/// Classify a `syn::Type` into an `FfiType`, or return a span-accurate error.
fn classify(ty: &syn::Type) -> syn::Result<FfiType> {
    match ty {
        syn::Type::Path(tp) if tp.qself.is_none() => {
            let seg = tp.path.segments.last().unwrap();
            let name = seg.ident.to_string();
            match name.as_str() {
                "i32" => Ok(FfiType::I32),
                "i64" => Ok(FfiType::I64),
                "u32" => Ok(FfiType::U32),
                "u64" => Ok(FfiType::U64),
                "f32" => Ok(FfiType::F32),
                "f64" => Ok(FfiType::F64),
                "bool" => Ok(FfiType::Bool),
                "String" => Ok(FfiType::OwnedString),
                _ => Ok(FfiType::Named(seg.ident.clone())),
            }
        }
        syn::Type::Reference(r) => {
            // Only &str (with any lifetime) is supported
            if let syn::Type::Path(tp) = r.elem.as_ref() {
                if tp.path.is_ident("str") {
                    return Ok(FfiType::StrRef);
                }
            }
            Err(syn::Error::new_spanned(
                ty,
                "#[furst_export]: only `&str` references are supported across FFI; \
                 consider using a primitive type or a #[furst_export] struct",
            ))
        }
        syn::Type::Ptr(_) => Ok(FfiType::OpaquePtr(ty.clone())),
        syn::Type::Tuple(t) if t.elems.is_empty() => Ok(FfiType::Unit),
        _ => Err(syn::Error::new_spanned(
            ty,
            "#[furst_export]: type is not FFI-safe; supported types are: \
             i32, i64, u32, u64, f32, f64, bool, &str, String, *mut T, *const T, \
             or a #[furst_export] struct/enum",
        )),
    }
}

// ─── Entry point ──────────────────────────────────────────────────────────

#[proc_macro_attribute]
pub fn furst_export(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let item2 = item.clone();

    // Try ItemFn first
    if let Ok(func) = syn::parse::<ItemFn>(item.clone()) {
        return expand_fn(func)
            .unwrap_or_else(|e| e.to_compile_error())
            .into();
    }

    // Try ItemStruct
    if let Ok(strct) = syn::parse::<ItemStruct>(item.clone()) {
        return expand_struct(strct)
            .unwrap_or_else(|e| e.to_compile_error())
            .into();
    }

    // Try ItemEnum
    if let Ok(enm) = syn::parse::<ItemEnum>(item.clone()) {
        return expand_enum(enm)
            .unwrap_or_else(|e| e.to_compile_error())
            .into();
    }

    // Try ItemImpl
    if let Ok(imp) = syn::parse::<ItemImpl>(item) {
        return expand_impl(imp)
            .unwrap_or_else(|e| e.to_compile_error())
            .into();
    }

    // Unsupported item kind
    syn::Error::new_spanned(
        TokenStream2::from(item2),
        "#[furst_export] can only be applied to fn, struct, enum, or impl",
    )
    .to_compile_error()
    .into()
}

// ─── Function export ───────────────────────────────────────────────────────

fn expand_fn(mut func: ItemFn) -> syn::Result<TokenStream2> {
    // Reject generics and async
    if !func.sig.generics.params.is_empty() {
        return Err(syn::Error::new_spanned(
            &func.sig.generics,
            "#[furst_export]: FFI functions cannot have generic parameters",
        ));
    }
    if func.sig.asyncness.is_some() {
        return Err(syn::Error::new_spanned(
            func.sig.asyncness,
            "#[furst_export]: FFI functions cannot be async",
        ));
    }

    // If already `extern "C"`, just add #[no_mangle] and return unchanged
    if is_extern_c(&func.sig.abi) {
        let attrs = &func.attrs;
        let vis = &func.vis;
        let sig = &func.sig;
        let body = &func.block;
        return Ok(quote! {
            #(#attrs)*
            #[no_mangle]
            #vis #sig #body
        });
    }

    let original_name = func.sig.ident.clone();
    let inner_name = format_ident!("__furst_inner_{}", original_name);

    // Rename the original function and strip #[furst_export] so it doesn't recurse
    func.sig.ident = inner_name.clone();
    func.attrs.retain(|a| !a.path().is_ident("furst_export"));

    // Classify return type
    let ret_ffi = match &func.sig.output {
        syn::ReturnType::Default => FfiType::Unit,
        syn::ReturnType::Type(_, ty) => classify(ty)?,
    };

    // Build C wrapper params, let-bindings for &str reconstruction, call args
    let mut c_params: Vec<TokenStream2> = Vec::new();
    let mut let_bindings: Vec<TokenStream2> = Vec::new();
    let mut call_args: Vec<TokenStream2> = Vec::new();

    for input in &func.sig.inputs {
        let syn::FnArg::Typed(pat_type) = input else {
            return Err(syn::Error::new_spanned(
                input,
                "#[furst_export]: `self` parameters are not supported",
            ));
        };

        let pat = &pat_type.pat;
        let ffi_ty = classify(&pat_type.ty)?;

        match ffi_ty {
            FfiType::StrRef => {
                // Extract the parameter name from the pattern
                let name = pat_ident(pat)?;
                let ptr_name = format_ident!("{}_ptr", name);
                let len_name = format_ident!("{}_len", name);

                c_params.push(quote! { #ptr_name: *const u8 });
                c_params.push(quote! { #len_name: usize });

                let_bindings.push(quote! {
                    let #name: &str = unsafe {
                        ::core::str::from_utf8_unchecked(
                            ::core::slice::from_raw_parts(#ptr_name, #len_name)
                        )
                    };
                });

                call_args.push(quote! { #name });
            }
            FfiType::OwnedString => {
                return Err(syn::Error::new_spanned(
                    &pat_type.ty,
                    "#[furst_export]: `String` is only supported as a return type, not a parameter; \
                     use `&str` for string input",
                ));
            }
            _ => {
                let c_ty = ffi_type_to_rust_tokens(&ffi_ty);
                c_params.push(quote! { #pat: #c_ty });
                call_args.push(quote! { #pat });
            }
        }
    }

    // Build return type and wrapping expression
    let (c_ret_ty, wrap_result): (TokenStream2, Box<dyn Fn(TokenStream2) -> TokenStream2>) =
        match &ret_ffi {
            FfiType::Unit => (quote! {}, Box::new(|call| quote! { #call; })),
            FfiType::OwnedString => (
                quote! { -> ::furst_rt::FurstStr },
                Box::new(|call| quote! { ::furst_rt::FurstStr::from(#call) }),
            ),
            _ => {
                let ty = ffi_type_to_rust_tokens(&ret_ffi);
                (quote! { -> #ty }, Box::new(move |call| quote! { #call }))
            }
        };

    let inner_call = quote! { #inner_name(#(#call_args),*) };
    let body = wrap_result(inner_call);

    // func already carries its (cleaned) attrs + vis — emit it directly
    Ok(quote! {
        #[allow(non_snake_case, dead_code)]
        #func

        #[no_mangle]
        pub extern "C" fn #original_name(#(#c_params),*) #c_ret_ty {
            #(#let_bindings)*
            #body
        }
    })
}

fn is_extern_c(abi: &Option<syn::Abi>) -> bool {
    match abi {
        Some(syn::Abi {
            name: Some(name), ..
        }) => name.value() == "C",
        _ => false,
    }
}

fn pat_ident(pat: &syn::Pat) -> syn::Result<&syn::Ident> {
    match pat {
        syn::Pat::Ident(pi) => Ok(&pi.ident),
        _ => Err(syn::Error::new_spanned(
            pat,
            "#[furst_export]: only simple identifier patterns are supported in function parameters",
        )),
    }
}

/// Map an `FfiType` to its Rust token representation for use in generated code.
fn ffi_type_to_rust_tokens(ty: &FfiType) -> TokenStream2 {
    match ty {
        FfiType::I32 => quote! { i32 },
        FfiType::I64 => quote! { i64 },
        FfiType::U32 => quote! { u32 },
        FfiType::U64 => quote! { u64 },
        FfiType::F32 => quote! { f32 },
        FfiType::F64 => quote! { f64 },
        FfiType::Bool => quote! { bool },
        FfiType::Unit => quote! { () },
        FfiType::StrRef => quote! { *const u8 }, // shouldn't reach here
        FfiType::OwnedString => quote! { ::furst_rt::FurstStr },
        FfiType::Named(ident) => quote! { #ident },
        FfiType::OpaquePtr(ty) => quote! { #ty },
    }
}

// ─── Struct export ─────────────────────────────────────────────────────────

fn expand_struct(mut strct: ItemStruct) -> syn::Result<TokenStream2> {
    // Reject generics
    if !strct.generics.params.is_empty() {
        return Err(syn::Error::new_spanned(
            &strct.generics,
            "#[furst_export]: FFI structs cannot have generic parameters",
        ));
    }

    // Validate all fields are FFI-safe
    for field in &strct.fields {
        classify(&field.ty)?;
    }

    // Add #[repr(C)] if not already present
    if !has_repr_c(&strct.attrs) {
        let repr_c: syn::Attribute = syn::parse_quote!(#[repr(C)]);
        strct.attrs.insert(0, repr_c);
    }

    Ok(quote! { #strct })
}

fn has_repr_c(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|a| {
        if !a.path().is_ident("repr") {
            return false;
        }
        // Parse the repr(...) argument list
        let Ok(meta_list) = a.meta.require_list() else {
            return false;
        };
        // Check if any token in the list is the ident "C"
        meta_list.tokens.clone().into_iter().any(|tok| {
            matches!(tok, proc_macro2::TokenTree::Ident(i) if i == "C")
        })
    })
}

// ─── Enum export ───────────────────────────────────────────────────────────

fn expand_enum(mut enm: ItemEnum) -> syn::Result<TokenStream2> {
    // Reject generics
    if !enm.generics.params.is_empty() {
        return Err(syn::Error::new_spanned(
            &enm.generics,
            "#[furst_export]: FFI enums cannot have generic parameters",
        ));
    }

    let is_tagged = enm.variants.iter().any(|v| !v.fields.is_empty());

    if !is_tagged {
        // C-style enum: add #[repr(i32)]
        if !has_repr_i32(&enm.attrs) {
            let repr: syn::Attribute = syn::parse_quote!(#[repr(i32)]);
            enm.attrs.insert(0, repr);
        }
        return Ok(quote! { #enm });
    }

    // Tagged union enum
    expand_tagged_enum(enm)
}

fn has_repr_i32(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|a| {
        if !a.path().is_ident("repr") {
            return false;
        }
        let Ok(meta_list) = a.meta.require_list() else {
            return false;
        };
        meta_list.tokens.clone().into_iter().any(|tok| {
            matches!(tok, proc_macro2::TokenTree::Ident(i) if i == "i32")
        })
    })
}

fn expand_tagged_enum(enm: ItemEnum) -> syn::Result<TokenStream2> {
    let enum_name = &enm.ident;
    let vis = &enm.vis;

    let tag_name = format_ident!("{}Tag", enum_name);
    let union_name = format_ident!("{}Union", enum_name);
    let ffi_name = format_ident!("{}Ffi", enum_name);

    let mut tag_variants: Vec<TokenStream2> = Vec::new();
    let mut data_structs: Vec<TokenStream2> = Vec::new();
    let mut union_fields: Vec<TokenStream2> = Vec::new();
    let mut from_arms: Vec<TokenStream2> = Vec::new();

    for (i, variant) in enm.variants.iter().enumerate() {
        let vname = &variant.ident;
        let disc = i as i32;

        // Only named fields are supported
        match &variant.fields {
            syn::Fields::Unit => {
                // Unit variant in a tagged enum — treat as empty data struct
                let data_name = format_ident!("{}{}Data", enum_name, vname);
                data_structs.push(quote! {
                    #[repr(C)]
                    #[derive(Clone, Copy)]
                    #vis struct #data_name;
                });
                let field_name = format_ident!("{}", to_snake(vname.to_string()));
                union_fields.push(quote! {
                    pub #field_name: #data_name
                });
                tag_variants.push(quote! { #vname = #disc });
                from_arms.push(quote! {
                    #enum_name::#vname => #ffi_name {
                        tag: #tag_name::#vname,
                        data: #union_name { #field_name: #data_name },
                    }
                });
            }
            syn::Fields::Named(named) => {
                let data_name = format_ident!("{}{}Data", enum_name, vname);

                // Validate and collect fields
                let mut struct_fields: Vec<TokenStream2> = Vec::new();
                let mut field_names: Vec<&syn::Ident> = Vec::new();
                for field in &named.named {
                    let fname = field.ident.as_ref().unwrap();
                    let fty = &field.ty;
                    classify(fty)?; // validates FFI safety
                    struct_fields.push(quote! { pub #fname: #fty });
                    field_names.push(fname);
                }

                data_structs.push(quote! {
                    #[repr(C)]
                    #[derive(Clone, Copy)]
                    #vis struct #data_name {
                        #(#struct_fields),*
                    }
                });

                let snake_vname = format_ident!("{}", to_snake(vname.to_string()));
                union_fields.push(quote! {
                    pub #snake_vname: #data_name
                });
                tag_variants.push(quote! { #vname = #disc });
                from_arms.push(quote! {
                    #enum_name::#vname { #(#field_names),* } => #ffi_name {
                        tag: #tag_name::#vname,
                        data: #union_name {
                            #snake_vname: #data_name { #(#field_names),* }
                        },
                    }
                });
            }
            syn::Fields::Unnamed(_) => {
                return Err(syn::Error::new_spanned(
                    &variant.fields,
                    "#[furst_export]: tuple variants are not supported for FFI export; \
                     use named fields instead: `Variant { field: Type }`",
                ));
            }
        }
    }

    Ok(quote! {
        // Keep the original Rust enum for ergonomic use in Rust code
        #enm

        // Tag enum — discriminant for the active variant
        #[repr(i32)]
        #[derive(Clone, Copy, PartialEq, Eq, Debug)]
        #vis enum #tag_name {
            #(#tag_variants),*
        }

        // Per-variant data structs
        #(#data_structs)*

        // C union of all variant payloads
        #[repr(C)]
        #vis union #union_name {
            #(#union_fields),*
        }

        // Wrapper FFI struct: tag + union payload
        #[repr(C)]
        #vis struct #ffi_name {
            pub tag: #tag_name,
            pub data: #union_name,
        }

        impl From<#enum_name> for #ffi_name {
            fn from(val: #enum_name) -> Self {
                match val {
                    #(#from_arms),*
                }
            }
        }
    })
}

// ─── Impl export ──────────────────────────────────────────────────────────

fn expand_impl(mut imp: ItemImpl) -> syn::Result<TokenStream2> {
    // Reject trait impls
    if imp.trait_.is_some() {
        return Err(syn::Error::new_spanned(
            &imp,
            "#[furst_export]: trait impls are not supported; \
             use `#[furst_export] impl TypeName { ... }` for inherent impls only",
        ));
    }

    // Reject generics
    if !imp.generics.params.is_empty() {
        return Err(syn::Error::new_spanned(
            &imp.generics,
            "#[furst_export]: generic impl blocks are not supported",
        ));
    }

    // Extract type name from self_ty
    let type_name = match imp.self_ty.as_ref() {
        syn::Type::Path(tp) if tp.qself.is_none() => {
            tp.path.segments.last().unwrap().ident.clone()
        }
        _ => {
            return Err(syn::Error::new_spanned(
                &imp.self_ty,
                "#[furst_export]: impl target must be a simple type name",
            ));
        }
    };

    let snake_prefix = to_snake(type_name.to_string());
    let mut ffi_fns: Vec<TokenStream2> = Vec::new();
    let mut has_free = false;

    for item in &imp.items {
        let syn::ImplItem::Fn(method) = item else {
            continue;
        };

        let method_name = &method.sig.ident;
        let ffi_name = format_ident!("{}_{}", snake_prefix, method_name);

        if method_name == "free" {
            has_free = true;
        }

        // Reject generics and async on methods
        if !method.sig.generics.params.is_empty() {
            return Err(syn::Error::new_spanned(
                &method.sig.generics,
                "#[furst_export]: FFI methods cannot have generic parameters",
            ));
        }
        if method.sig.asyncness.is_some() {
            return Err(syn::Error::new_spanned(
                method.sig.asyncness,
                "#[furst_export]: FFI methods cannot be async",
            ));
        }

        // Determine receiver kind: &self, &mut self, self: &Self, self: &mut Self
        let receiver = match method.sig.inputs.first() {
            Some(syn::FnArg::Receiver(r)) => {
                if r.reference.is_some() {
                    // Shorthand form: &self or &mut self
                    if r.mutability.is_some() {
                        Some(true) // &mut self
                    } else {
                        Some(false) // &self
                    }
                } else {
                    // Check for typed form: self: &Self or self: &mut Self
                    match r.ty.as_ref() {
                        syn::Type::Reference(ref_ty) => {
                            if ref_ty.mutability.is_some() {
                                Some(true) // self: &mut Self
                            } else {
                                Some(false) // self: &Self
                            }
                        }
                        _ => {
                            return Err(syn::Error::new_spanned(
                                r,
                                "#[furst_export]: `self` by value is not supported for opaque handles; \
                                 use `&self` or `&mut self`",
                            ));
                        }
                    }
                }
            }
            _ => None, // associated function (no self)
        };

        // Build C params
        let mut c_params: Vec<TokenStream2> = Vec::new();
        let mut let_bindings: Vec<TokenStream2> = Vec::new();
        let mut call_args: Vec<TokenStream2> = Vec::new();

        if let Some(is_mut) = receiver {
            if is_mut {
                c_params.push(quote! { this: *mut #type_name });
            } else {
                c_params.push(quote! { this: *const #type_name });
            }
        }

        // Process non-self params
        let params_iter = method.sig.inputs.iter().filter(|arg| {
            !matches!(arg, syn::FnArg::Receiver(_))
        });

        for input in params_iter {
            let syn::FnArg::Typed(pat_type) = input else {
                continue;
            };
            let pat = &pat_type.pat;
            let ffi_ty = classify(&pat_type.ty)?;

            match ffi_ty {
                FfiType::StrRef => {
                    let name = pat_ident(pat)?;
                    let ptr_name = format_ident!("{}_ptr", name);
                    let len_name = format_ident!("{}_len", name);
                    c_params.push(quote! { #ptr_name: *const u8 });
                    c_params.push(quote! { #len_name: usize });
                    let_bindings.push(quote! {
                        let #name: &str = unsafe {
                            ::core::str::from_utf8_unchecked(
                                ::core::slice::from_raw_parts(#ptr_name, #len_name)
                            )
                        };
                    });
                    call_args.push(quote! { #name });
                }
                FfiType::OwnedString => {
                    return Err(syn::Error::new_spanned(
                        &pat_type.ty,
                        "#[furst_export]: `String` is only supported as a return type",
                    ));
                }
                _ => {
                    let c_ty = ffi_type_to_rust_tokens(&ffi_ty);
                    c_params.push(quote! { #pat: #c_ty });
                    call_args.push(quote! { #pat });
                }
            }
        }

        // Classify return type, handling Self → *mut TypeName
        let ret_is_self = match &method.sig.output {
            syn::ReturnType::Type(_, ty) => {
                matches!(ty.as_ref(), syn::Type::Path(tp)
                    if tp.qself.is_none() && tp.path.is_ident("Self"))
            }
            _ => false,
        };

        let ret_ffi = if ret_is_self {
            FfiType::OpaquePtr(syn::parse_quote! { *mut #type_name })
        } else {
            match &method.sig.output {
                syn::ReturnType::Default => FfiType::Unit,
                syn::ReturnType::Type(_, ty) => classify(ty)?,
            }
        };

        // Build the inner call expression
        let inner_call = match receiver {
            Some(true) => {
                // &mut self
                quote! { unsafe { &mut *this }.#method_name(#(#call_args),*) }
            }
            Some(false) => {
                // &self
                quote! { unsafe { &*this }.#method_name(#(#call_args),*) }
            }
            None => {
                // associated function
                quote! { #type_name::#method_name(#(#call_args),*) }
            }
        };

        // Build return type and wrapping
        let (c_ret_ty, body) = if ret_is_self {
            (
                quote! { -> *mut #type_name },
                quote! {
                    #(#let_bindings)*
                    Box::into_raw(Box::new(#inner_call))
                },
            )
        } else {
            match &ret_ffi {
                FfiType::Unit => (quote! {}, quote! { #(#let_bindings)* #inner_call; }),
                FfiType::OwnedString => (
                    quote! { -> ::furst_rt::FurstStr },
                    quote! { #(#let_bindings)* ::furst_rt::FurstStr::from(#inner_call) },
                ),
                _ => {
                    let ty = ffi_type_to_rust_tokens(&ret_ffi);
                    (
                        quote! { -> #ty },
                        quote! { #(#let_bindings)* #inner_call },
                    )
                }
            }
        };

        ffi_fns.push(quote! {
            #[no_mangle]
            pub extern "C" fn #ffi_name(#(#c_params),*) #c_ret_ty {
                #body
            }
        });
    }

    // Auto-generate free if not explicitly defined
    if !has_free {
        let free_name = format_ident!("{}_free", snake_prefix);
        ffi_fns.push(quote! {
            #[no_mangle]
            pub extern "C" fn #free_name(this: *mut #type_name) {
                if !this.is_null() {
                    unsafe { drop(Box::from_raw(this)); }
                }
            }
        });
    }

    // Strip #[furst_export] from the impl block to avoid recursion
    imp.attrs.retain(|a| !a.path().is_ident("furst_export"));

    Ok(quote! {
        #imp

        #(#ffi_fns)*
    })
}

/// Convert PascalCase identifier to snake_case for union field names.
fn to_snake(s: String) -> String {
    let mut out = String::new();
    for (i, ch) in s.char_indices() {
        if ch.is_uppercase() && i > 0 {
            out.push('_');
        }
        out.push(ch.to_lowercase().next().unwrap());
    }
    out
}
