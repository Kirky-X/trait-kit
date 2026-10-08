// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! Proc-macro crate for trait-kit.
//!
//! Provides three derives:
//!
//! - [`derive(Module)`](derive@Module) — derive `trait_kit::core::ModuleMeta`
//!   for a struct, generating the module name and dependency declarations
//!   without hand-writing the impl (or calling the `impl_module_meta!` macro).
//! - [`derive(ConfigInherit)`](derive@ConfigInherit) — compile-time safe
//!   field-level override type + `ConfigInherit` impl for config inheritance
//!   (migrated from the retired `trait-kit-derive` crate).
//! - [`derive(SharedConfig)`](derive@SharedConfig) — `extract_shared` /
//!   `inject_shared` for cross-type shared field inheritance (same origin).
//!
//! # Attributes
//!
//! The helper attribute is `#[module(...)]` on the struct:
//!
//! - `name = "literal"` — override the diagnostic module name. Default: the
//!   struct identifier verbatim.
//! - `deps(TypeA, TypeB)` (or `deps = [TypeA, TypeB]`) — declare dependencies
//!   as a list of module types implementing `ModuleMeta`. Each dependency
//!   contributes `(Dep::NAME, TypeId::of::<Dep>())`, byte-for-byte identical
//!   to the hand-written form.
//!
//! # Example
//!
//! ```ignore
//! use trait_kit_macros::Module;
//!
//! #[derive(Module)]
//! #[module(name = "db", deps(LoggerModule))]
//! struct DbModule;
//!
//! // Equivalent to the hand-written impl:
//! // impl ModuleMeta for DbModule {
//! //     const NAME: &'static str = "db";
//! //     fn dependencies() -> &'static [(&'static str, TypeId)] {
//! //         &[(LoggerModule::NAME, TypeId::of::<LoggerModule>())]
//! //     }
//! // }
//! ```

use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::parse::{Parse, ParseStream};
use syn::spanned::Spanned;
use syn::{
    Data, DeriveInput, Fields, Ident, LitStr, Meta, Token, Type, bracketed, parenthesized,
    parse_macro_input, token,
};

/// Parsed contents of the `#[module(...)]` helper attribute.
#[derive(Default)]
struct ModuleArgs {
    name: Option<LitStr>,
    deps: Vec<Type>,
}

impl Parse for ModuleArgs {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let mut args = Self::default();

        while !input.is_empty() {
            let key: Ident = input.parse()?;
            match key.to_string().as_str() {
                "name" => {
                    input.parse::<Token![=]>()?;
                    args.name = Some(input.parse()?);
                }
                "deps" => {
                    let content;
                    if input.peek(token::Paren) {
                        parenthesized!(content in input);
                    } else if input.peek(Token![=]) {
                        input.parse::<Token![=]>()?;
                        bracketed!(content in input);
                    } else {
                        return Err(syn::Error::new(
                            key.span(),
                            "expected `deps(A, B)` or `deps = [A, B]`",
                        ));
                    }
                    while !content.is_empty() {
                        args.deps.push(content.parse()?);
                        if !content.is_empty() {
                            content.parse::<Token![,]>()?;
                        }
                    }
                }
                other => {
                    return Err(syn::Error::new(
                        key.span(),
                        format!("unknown `module` argument `{other}`; expected `name` or `deps`"),
                    ));
                }
            }
            if !input.is_empty() {
                input.parse::<Token![,]>()?;
            }
        }

        Ok(args)
    }
}

/// Derive `trait_kit::core::ModuleMeta` for a struct.
///
/// See the [crate docs](self) for the `#[module(...)]` attribute grammar.
///
/// # Compile errors
///
/// - Unknown `#[module(...)]` arguments fail with a spanned, named error.
/// - Generic structs are not supported (module types must be concrete for
///   `TypeId::of`) — a clear error is emitted instead of a confusing
///   unresolved-type failure.
#[proc_macro_derive(ConfigInherit, attributes(config_inherit))]
pub fn derive_config_inherit(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;
    let vis = &input.vis;

    // Parse container-level #[config_inherit(...)] attributes
    let mut override_name = format_ident!("{name}Override");
    for attr in &input.attrs {
        if !attr.path().is_ident("config_inherit") {
            continue;
        }
        if let Ok(meta_list) = attr.meta.require_list() {
            let nested = meta_list
                .parse_args_with(
                    syn::punctuated::Punctuated::<Meta, syn::Token![,]>::parse_terminated,
                )
                .expect("failed to parse config_inherit attributes");
            for meta in nested {
                if let Meta::NameValue(nv) = meta
                    && nv.path.is_ident("name")
                    && let syn::Expr::Lit(lit) = &nv.value
                    && let syn::Lit::Str(s) = &lit.lit
                {
                    override_name = format_ident!("{}", s.value());
                }
            }
        }
    }

    let fields = match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(named) => &named.named,
            _ => panic!("ConfigInherit only supports structs with named fields"),
        },
        _ => panic!("ConfigInherit only supports structs"),
    };

    let mut override_fields = Vec::new();
    let mut apply_stmts = Vec::new();

    for field in fields {
        let field_name = field.ident.as_ref().expect("named field");
        let field_ty = &field.ty;

        let is_nested = field.attrs.iter().any(|attr| {
            if !attr.path().is_ident("config_inherit") {
                return false;
            }
            if let Ok(meta_list) = attr.meta.require_list() {
                let nested = meta_list
                    .parse_args_with(
                        syn::punctuated::Punctuated::<Meta, syn::Token![,]>::parse_terminated,
                    )
                    .expect("failed to parse config_inherit field attributes");
                nested
                    .iter()
                    .any(|m| matches!(m, Meta::Path(p) if p.is_ident("nested")))
            } else {
                false
            }
        });

        if is_nested {
            // Nested: Override field is Option<<FieldType as ConfigInherit>::Override>
            override_fields.push(quote! {
                pub #field_name: Option<<#field_ty as trait_kit::kit::ConfigInherit>::Override>
            });
            apply_stmts.push(quote! {
                if let Some(ref nested_ovr) = ovr.#field_name {
                    trait_kit::kit::ConfigInherit::apply_override(&mut self.#field_name, nested_ovr);
                }
            });
        } else {
            // Regular: Override field is Option<FieldType>
            override_fields.push(quote! {
                pub #field_name: Option<#field_ty>
            });
            apply_stmts.push(quote! {
                if let Some(ref val) = ovr.#field_name {
                    self.#field_name = val.clone();
                }
            });
        }
    }

    let expanded = quote! {
        /// Auto-generated Override type for `ConfigInherit`.
        #[derive(Clone, Default)]
        #vis struct #override_name {
            #(#override_fields,)*
        }

        impl trait_kit::kit::ConfigInherit for #name {
            type Override = #override_name;

            fn apply_override(&mut self, ovr: &Self::Override) {
                #(#apply_stmts;)*
            }
        }
    };

    TokenStream::from(expanded)
}

/// Derive `SharedConfig` for a struct.
///
/// Generates `extract_shared` and `inject_shared` implementations based on
/// the `#[shared(field1, field2, ...)]` attribute listing which fields
/// participate in the shared namespace.
///
/// # Attributes
///
/// - `#[shared(host, port)]` — declares `host` and `port` as shared fields.
///   Only listed fields are extracted/injected.
///
/// # Example
///
/// ```ignore
/// #[derive(Clone, SharedConfig)]
/// #[shared(host, port)]
/// struct DbConfig {
///     host: String,
///     port: u16,
///     max_connections: u32, // not shared
/// }
/// ```
#[proc_macro_derive(SharedConfig, attributes(shared))]
pub fn derive_shared_config(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;

    // Parse #[shared(field1, field2, ...)] attribute
    let mut shared_field_names: Vec<syn::Ident> = Vec::new();
    for attr in &input.attrs {
        if !attr.path().is_ident("shared") {
            continue;
        }
        let nested = attr
            .parse_args_with(
                syn::punctuated::Punctuated::<syn::Ident, syn::Token![,]>::parse_terminated,
            )
            .expect("expected #[shared(field1, field2, ...)]");
        shared_field_names.extend(nested);
    }

    if shared_field_names.is_empty() {
        panic!("SharedConfig requires at least one field in #[shared(...)]");
    }

    let fields = match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(named) => &named.named,
            _ => panic!("SharedConfig only supports structs with named fields"),
        },
        _ => panic!("SharedConfig only supports structs"),
    };

    // Build a map from field name to field type for validation
    let field_map: std::collections::HashMap<String, &syn::Type> = fields
        .iter()
        .filter_map(|f| f.ident.as_ref().map(|ident| (ident.to_string(), &f.ty)))
        .collect();

    // Validate that all shared field names exist in the struct
    for field_name in &shared_field_names {
        if !field_map.contains_key(&field_name.to_string()) {
            panic!(
                "SharedConfig: field `{field_name}` listed in #[shared(...)] does not exist in struct `{name}`"
            );
        }
    }

    // Generate extract_shared: serialize each shared field to JSON
    let extract_stmts: Vec<_> = shared_field_names
        .iter()
        .map(|field_name| {
            let key = field_name.to_string();
            quote! {
                map.insert(
                    #key.into(),
                    serde_json::to_value(&self.#field_name)
                        .expect("SharedConfig: failed to serialize shared field"),
                );
            }
        })
        .collect();

    // Generate inject_shared: deserialize each shared field from JSON
    let inject_stmts: Vec<_> = shared_field_names
        .iter()
        .map(|field_name| {
            let key = field_name.to_string();
            quote! {
                if let Some(val) = shared.get(#key) {
                    if let Ok(v) = serde_json::from_value(val.clone()) {
                        self.#field_name = v;
                    }
                    // Type mismatch: silently skip (preserve original value)
                }
            }
        })
        .collect();

    let expanded = quote! {
        impl trait_kit::kit::SharedConfig for #name {
            fn extract_shared(&self) -> serde_json::Map<String, serde_json::Value> {
                let mut map = serde_json::Map::new();
                #(#extract_stmts)*
                map
            }

            fn inject_shared(&mut self, shared: &serde_json::Map<String, serde_json::Value>) {
                #(#inject_stmts)*
            }
        }
    };

    TokenStream::from(expanded)
}

#[proc_macro_derive(Module, attributes(module))]
pub fn derive_module(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as syn::DeriveInput);
    match expand_module(&input) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.to_compile_error().into(),
    }
}

fn expand_module(input: &syn::DeriveInput) -> syn::Result<proc_macro2::TokenStream> {
    // Module types must be concrete: `TypeId::of::<M>()` requires `M: 'static`
    // without generic parameters. Reject generics up front with a clear error.
    if !input.generics.params.is_empty() {
        return Err(syn::Error::new(
            input.generics.span(),
            "#[derive(Module)] does not support generic structs: module types must be \
             concrete so they can be keyed by TypeId in the Kit dependency graph",
        ));
    }

    // Collect the `#[module(...)]` helper attributes (multiple merge; last
    // `name` wins, deps accumulate).
    let mut args = ModuleArgs::default();
    for attr in &input.attrs {
        if attr.path().is_ident("module") {
            let parsed: ModuleArgs = attr.parse_args()?;
            if parsed.name.is_some() {
                args.name = parsed.name;
            }
            args.deps.extend(parsed.deps);
        }
    }

    let ident = &input.ident;
    let name = match &args.name {
        Some(lit) => quote! { #lit },
        None => quote! { stringify!(#ident) },
    };

    // Emit the `static DEPS` form used by `impl_module_meta!` so the generated
    // code is byte-for-byte comparable to the hand-written impl (the array
    // literal itself cannot be `&'static`-promoted in general).
    let dep_entries = args.deps.iter().map(|dep| {
        quote! {
            (<#dep as ::trait_kit::core::ModuleMeta>::NAME, ::std::any::TypeId::of::<#dep>())
        }
    });

    Ok(quote! {
        impl ::trait_kit::core::ModuleMeta for #ident {
            const NAME: &'static str = #name;

            fn dependencies() -> &'static [(&'static str, ::std::any::TypeId)] {
                static DEPS: &[(&str, ::std::any::TypeId)] = &[ #(#dep_entries,)* ];
                DEPS
            }
        }
    })
}
