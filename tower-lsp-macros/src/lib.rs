//! Internal procedural macros for [`tower-lsp`](https://docs.rs/tower-lsp).
//!
//! This crate should not be used directly.

extern crate proc_macro;

use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, FnArg, ItemTrait, LitStr, ReturnType, TraitItem};

/// Macro for generating LSP server implementation from [`lsp-types`](https://docs.rs/lsp-types).
///
/// This procedural macro annotates the `tower_lsp::LanguageServer` trait and generates a
/// corresponding `register_lsp_methods()` function which registers all the methods on that trait
/// as RPC handlers.
#[proc_macro_attribute]
pub fn rpc(attr: TokenStream, item: TokenStream) -> TokenStream {
    // Attribute will be parsed later in `parse_method_calls()`.
    if !attr.is_empty() {
        return item;
    }

    let lang_server_trait = parse_macro_input!(item as ItemTrait);
    let method_calls = parse_method_calls(&lang_server_trait);
    let req_types_and_router_fn = gen_server_router(&lang_server_trait.ident, &method_calls);

    let tokens = quote! {
        #lang_server_trait
        #req_types_and_router_fn
    };

    tokens.into()
}

struct MethodCall<'a> {
    rpc_name: String,
    handler_name: &'a syn::Ident,
    params: Option<&'a syn::Type>,
    result: Option<&'a syn::Type>,
}

fn parse_method_calls(lang_server_trait: &ItemTrait) -> Vec<MethodCall> {
    let mut calls = Vec::new();

    for item in &lang_server_trait.items {
        let method = match item {
            TraitItem::Fn(m) => m,
            _ => continue,
        };

        let attr = method
            .attrs
            .iter()
            .find(|attr| attr.meta.path().is_ident("rpc"))
            .expect("expected `#[rpc(name = \"foo\")]` attribute");

        let mut rpc_name = String::new();
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("name") {
                let s: LitStr = meta.value().and_then(|v| v.parse())?;
                rpc_name = s.value();
                Ok(())
            } else {
                Err(meta.error("expected `name` identifier in `#[rpc]`"))
            }
        })
        .unwrap();

        let params = method.sig.inputs.iter().nth(1).and_then(|arg| match arg {
            FnArg::Typed(pat) => Some(&*pat.ty),
            _ => None,
        });

        let result = match &method.sig.output {
            ReturnType::Default => None,
            ReturnType::Type(_, ty) => Some(&**ty),
        };

        calls.push(MethodCall {
            rpc_name,
            handler_name: &method.sig.ident,
            params,
            result,
        });
    }

    calls
}

fn gen_server_router(trait_name: &syn::Ident, methods: &[MethodCall]) -> proc_macro2::TokenStream {
    let route_registrations: proc_macro2::TokenStream = methods
        .iter()
        .map(|method| {
            let rpc_name = &method.rpc_name;
            let handler = &method.handler_name;

            let layer = match &rpc_name[..] {
                "initialize" => quote! { layers::Initialize::new(state.clone(), pending.clone()) },
                "shutdown" => quote! { layers::Shutdown::new(state.clone(), pending.clone()) },
                _ => quote! { layers::Normal::new(state.clone(), pending.clone()) },
            };

            // NOTE: In a perfect world, we could simply loop over each `MethodCall` and emit
            // `router.method(#rpc_name, S::#handler);` for each. While such an approach
            // works for inherent async functions and methods, it breaks with `async-trait` methods
            // due to this unfortunate `rustc` bug:
            //
            // https://github.com/rust-lang/rust/issues/64552
            //
            // As a workaround, we wrap each `async-trait` method in a regular `async fn` before
            // passing it to `.method`, as documented in this GitHub issue:
            //
            // https://github.com/dtolnay/async-trait/issues/167
            match (method.params, method.result) {
                (Some(params), Some(result)) => quote! {
                    async fn #handler<S: #trait_name>(server: &S, params: #params) -> #result {
                        server.#handler(params).await
                    }
                    router.method(#rpc_name, #handler, #layer);
                },
                (None, Some(result)) => quote! {
                    async fn #handler<S: #trait_name>(server: &S) -> #result {
                        server.#handler().await
                    }
                    router.method(#rpc_name, #handler, #layer);
                },
                (Some(params), None) => quote! {
                    async fn #handler<S: #trait_name>(server: &S, params: #params) {
                        server.#handler(params).await
                    }
                    router.method(#rpc_name, #handler, #layer);
                },
                (None, None) => quote! {
                    async fn #handler<S: #trait_name>(server: &S) {
                        server.#handler().await
                    }
                    router.method(#rpc_name, #handler, #layer);
                },
            }
        })
        .collect();

    quote! {
        mod generated {
            use std::sync::Arc;
            use std::future::{Future, Ready};

            use lsp_types::*;
            use lsp_types::notification::*;
            use lsp_types::request::*;
            use serde_json::Value;

            use super::#trait_name;
            use crate::jsonrpc::{Result, Router};
            use crate::service::{layers, Client, Pending, ServerState, State, ExitedError};

            fn cancel_request(params: CancelParams, p: &Pending) -> Ready<()> {
                p.cancel(&params.id.into());
                std::future::ready(())
            }

            pub(crate) fn register_lsp_methods<S>(
                mut router: Router<S, ExitedError>,
                state: Arc<ServerState>,
                pending: Arc<Pending>,
                client: Client,
            ) -> Router<S, ExitedError>
            where
                S: #trait_name,
            {
                #route_registrations

                let p = pending.clone();
                router.method(
                    "$/cancelRequest",
                    move |_: &S, params| cancel_request(params, &p),
                    tower::layer::util::Identity::new(),
                );
                router.method(
                    "exit",
                    |_: &S| std::future::ready(()),
                    layers::Exit::new(state.clone(), pending, client.clone()),
                );

                router
            }
        }
    }
}
