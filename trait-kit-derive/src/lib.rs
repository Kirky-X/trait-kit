// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! Derive macros for trait-kit config inheritance.
//!
//! - `#[derive(ConfigInherit)]` — generates an `Override` type and
//!   `ConfigInherit` impl for compile-time safe field-level override.
//! - `#[derive(SharedConfig)]` — generates `extract_shared` / `inject_shared`
//!   for cross-type shared field inheritance.

use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{Data, DeriveInput, Fields, Meta, parse_macro_input};

/// Derive `ConfigInherit` for a struct.
///
/// Generates a companion `XxxOverride` struct where each field is `Option<T>`,
/// plus a `ConfigInherit` impl that applies only `Some` fields.
///
/// # Attributes
///
/// - `#[config_inherit(name = "CustomOverrideName")]` — use a custom name for
///   the generated Override type instead of the default `{StructName}Override`.
/// - `#[config_inherit(nested)]` on a field — the field's type must itself
///   implement `ConfigInherit`. The Override field becomes
///   `Option<<FieldType as ConfigInherit>::Override>` and `apply_override`
///   delegates recursively.
///
/// # Example
///
/// ```ignore
/// #[derive(ConfigInherit)]
/// struct DbConfig {
///     host: String,
///     port: u16,
///     #[config_inherit(nested)]
///     pool: PoolConfig,
/// }
/// ```
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
