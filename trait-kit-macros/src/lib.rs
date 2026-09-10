// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! Proc-macro crate for trait-kit (T203).
//!
//! Currently provides [`derive(Module)`](derive@Module): derive
//! `trait_kit::core::ModuleMeta` for a struct, generating the module name and
//! dependency declarations without hand-writing the impl (or calling the
//! `impl_module_meta!` macro).
//!
//! # Attributes
//!
//! The helper attribute is `#[module(...)]` on the struct:
//!
//! - `name = "literal"` — override the diagnostic module name. Default: the
//!   struct identifier verbatim.
//! - `deps(TypeA, TypeB)` (or `deps = [TypeA, TypeB]`) — declare dependencies
//!   as a list of module types implementing [`ModuleMeta`]. Each dependency
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
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::spanned::Spanned;
use syn::{parenthesized, bracketed, parse_macro_input, token, Ident, LitStr, Token, Type};

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
