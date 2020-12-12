//! Internal procedural macros for [`tower-lsp`](https://docs.rs/tower-lsp).
//!
//! This crate should not be used directly.

extern crate proc_macro;

use heck::CamelCase;
use proc_macro::TokenStream;
use quote::quote;
use syn::{
    parse_macro_input, AttributeArgs, FnArg, ItemTrait, Lit, Meta, MetaNameValue, NestedMeta,
    ReturnType, TraitItem,
};

/// Macro for generating LSP server implementation from [`lsp-types`](https://docs.rs/lsp-types).
///
/// This procedural macro annotates the `tower_lsp::LanguageServer` trait and generates a
/// corresponding opaque `ServerRequest` struct along with a `handle_request()` function.
#[proc_macro_attribute]
pub fn rpc(attr: TokenStream, item: TokenStream) -> TokenStream {
    let attr_args = parse_macro_input!(attr as AttributeArgs);

    match attr_args.as_slice() {
        [] => {}
        [NestedMeta::Meta(meta)] if meta.path().is_ident("name") => return item,
        _ => panic!("unexpected attribute arguments"),
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

fn parse_method_calls<'a>(lang_server_trait: &'a ItemTrait) -> Vec<MethodCall<'a>> {
    let mut calls = Vec::new();

    for item in &lang_server_trait.items {
        let method = match item {
            TraitItem::Method(m) => m,
            _ => continue,
        };

        let rpc_name = method
            .attrs
            .iter()
            .filter_map(|attr| attr.parse_args::<Meta>().ok())
            .filter(|meta| meta.path().is_ident("name"))
            .find_map(|meta| match meta {
                Meta::NameValue(MetaNameValue {
                    lit: Lit::Str(lit), ..
                }) => Some(lit.value().trim_matches('"').to_owned()),
                _ => panic!("expected string literal for `#[rpc(name = ???)]` attribute"),
            })
            .expect("expected `#[rpc(name = \"foo\")]` attribute");

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
    let variant_names: Vec<syn::Ident> = methods
        .iter()
        .map(|method| syn::parse_str(&method.handler_name.to_string().to_camel_case()).unwrap())
        .collect();

    let variants: proc_macro2::TokenStream = methods
        .iter()
        .zip(variant_names.iter())
        .map(|(method, var_name)| {
            let rpc_name = &method.rpc_name;
            let variant = match (method.result.is_some(), method.params) {
                (true, Some(p)) => quote!(#var_name { params: Params<#p>, id: Id },),
                (true, None) => quote!(#var_name { id: Id },),
                (false, Some(p)) => quote!(#var_name { params: Params<#p> },),
                (false, None) => quote!(#var_name,),
            };

            quote! {
                #[serde(rename = #rpc_name)]
                #variant
            }
        })
        .collect();

    let id_match_arms: proc_macro2::TokenStream = methods
        .iter()
        .zip(variant_names.iter())
        .filter_map(|(method, var_name)| match method.result {
            Some(_) => Some(quote!(ServerMethod::#var_name { ref id, .. } => Some(id),)),
            None => None,
        })
        .collect();

    let route_match_arms: proc_macro2::TokenStream = methods
        .iter()
        .zip(variant_names.iter())
        .map(|(method, var_name)| {
            let rpc_name = method.rpc_name.as_str();
            let handler = &method.handler_name;
            match (method.result.is_some(), method.params.is_some()) {
                (true, true) if rpc_name == "initialize" => quote! {
                    (ServerMethod::#var_name { params: Valid(p), id }, State::Uninitialized) => {
                        state.set(State::Initializing);
                        let state = state.clone();
                        Box::pin(async move {
                            let res = match server.#handler(p).await {
                                Ok(result) => {
                                    let result = serde_json::to_value(result).unwrap();
                                    info!("language server initialized");
                                    state.set(State::Initialized);
                                    Response::ok(id, result)
                                }
                                Err(error) => {
                                    state.set(State::Uninitialized);
                                    Response::error(Some(id), error)
                                },
                            };

                            Ok(Some(Outgoing::Response(res)))
                        })
                    }
                    (ServerMethod::#var_name { params: Invalid(e), id }, State::Uninitialized) => {
                        error!("invalid parameters for {:?} request", #rpc_name);
                        let res = Response::error(Some(id), Error::invalid_params(e));
                        future::ok(Some(Outgoing::Response(res))).boxed()
                    }
                    (ServerMethod::#var_name { id, .. }, State::Initializing) => {
                        warn!("received duplicate `initialize` request, ignoring");
                        let res = Response::error(Some(id), Error::invalid_request());
                        future::ok(Some(Outgoing::Response(res))).boxed()
                    }
                },
                (true, false) if rpc_name == "shutdown" => quote! {
                    (ServerMethod::#var_name { id }, State::Initialized) => {
                        info!("shutdown request received, shutting down");
                        state.set(State::ShutDown);
                        pending
                            .execute(id, async move { server.#handler().await })
                            .map(|v| Ok(Some(Outgoing::Response(v))))
                            .boxed()
                    }
                },
                (true, true) => quote! {
                    (ServerMethod::#var_name { params: Valid(p), id }, State::Initialized) => {
                        pending
                            .execute(id, async move { server.#handler(p).await })
                            .map(|v| Ok(Some(Outgoing::Response(v))))
                            .boxed()
                    }
                    (ServerMethod::#var_name { params: Invalid(e), id }, State::Initialized) => {
                        error!("invalid parameters for {:?} request", #rpc_name);
                        let res = Response::error(Some(id), Error::invalid_params(e));
                        future::ok(Some(Outgoing::Response(res))).boxed()
                    }
                },
                (true, false) => quote! {
                    (ServerMethod::#var_name { id }, State::Initialized) => {
                        pending
                            .execute(id, async move { server.#handler().await })
                            .map(|v| Ok(Some(Outgoing::Response(v))))
                            .boxed()
                    }
                },
                (false, true) => quote! {
                    (ServerMethod::#var_name { params: Valid(p) }, State::Initialized) => {
                        Box::pin(async move { server.#handler(p).await; Ok(None) })
                    }
                    (ServerMethod::#var_name { .. }, State::Initialized) => {
                        warn!("invalid parameters for {:?} notification", #rpc_name);
                        future::ok(None).boxed()
                    }
                },
                (false, false) => quote! {
                    (ServerMethod::#var_name, State::Initialized) => {
                        Box::pin(async move { server.#handler().await; Ok(None) })
                    }
                },
            }
        })
        .collect();

    quote! {
        mod generated_impl {
            use std::future::Future;
            use std::pin::Pin;
            use std::sync::Arc;

            use futures::{future, FutureExt};
            use log::{error, info, warn};
            use lsp_types::*;
            use lsp_types::request::{
                GotoDeclarationParams, GotoImplementationParams, GotoTypeDefinitionParams,
            };

            use super::{#trait_name, ServerState, State};
            use crate::jsonrpc::{
                not_initialized_error, Error, ErrorCode, Id, Outgoing, Response, ServerRequests,
                Version,
            };
            use crate::service::ExitedError;

            /// A client-to-server LSP request.
            #[derive(Clone, Debug, PartialEq, serde::Deserialize)]
            #[cfg_attr(test, derive(serde::Serialize))]
            pub struct ServerRequest {
                jsonrpc: Version,
                #[serde(flatten)]
                kind: RequestKind,
            }

            #[derive(Clone, Debug, PartialEq, serde::Deserialize)]
            #[cfg_attr(test, derive(serde::Serialize))]
            #[serde(untagged)]
            enum RequestKind {
                Valid(ServerMethod),
                Invalid { id: Option<Id>, method: String },
            }

            #[derive(Clone, Debug, PartialEq, serde::Deserialize)]
            #[cfg_attr(test, derive(serde::Serialize))]
            #[serde(tag = "method")]
            enum ServerMethod {
                #variants
                #[serde(rename = "$/cancelRequest")]
                CancelRequest { id: Id },
                #[serde(rename = "exit")]
                Exit,
            }

            impl ServerMethod {
                fn id(&self) -> Option<&Id> {
                    match *self {
                        #id_match_arms
                        _ => None,
                    }
                }
            }

            #[derive(Clone, Debug, PartialEq)]
            #[cfg_attr(test, derive(serde::Serialize))]
            enum Params<T> {
                Valid(T),
                #[cfg_attr(test, serde(skip_serializing))]
                Invalid(String),
            }

            impl<'de, T: serde::Deserialize<'de>> serde::Deserialize<'de> for Params<T> {
                fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
                where
                    D: serde::Deserializer<'de>,
                {
                    match serde::Deserialize::deserialize(deserializer) {
                        Ok(Some(v)) => Ok(Params::Valid(v)),
                        Ok(None) => Ok(Params::Invalid("Missing params field".to_string())),
                        Err(e) => Ok(Params::Invalid(e.to_string())),
                    }
                }
            }

            pub(crate) fn handle_request<T: #trait_name>(
                server: T,
                state: &Arc<ServerState>,
                pending: &ServerRequests,
                request: ServerRequest,
            ) -> Pin<Box<dyn Future<Output = Result<Option<Outgoing>, ExitedError>> + Send>> {
                use Params::*;

                let method = match request.kind {
                    RequestKind::Valid(method) => method,
                    RequestKind::Invalid { id: Some(id), method } => {
                        error!("method {:?} not found", method);
                        let res = Response::error(Some(id), Error::method_not_found());
                        return future::ok(Some(Outgoing::Response(res))).boxed();
                    }
                    RequestKind::Invalid { id: None, method } if !method.starts_with("$/") => {
                        error!("method {:?} not found", method);
                        return future::ok(None).boxed();
                    }
                    RequestKind::Invalid { id: None, .. } => return future::ok(None).boxed(),
                };

                match (method, state.get()) {
                    #route_match_arms
                    (ServerMethod::CancelRequest { id }, State::Initialized) => {
                        pending.cancel(&id);
                        future::ok(None).boxed()
                    }
                    (ServerMethod::Exit, _) => {
                        info!("exit notification received, stopping");
                        state.set(State::Exited);
                        pending.cancel_all();
                        future::ok(None).boxed()
                    }
                    (other, State::Uninitialized) => Box::pin(match other.id().cloned() {
                        None => future::ok(None),
                        Some(id) => {
                            let res = Response::error(Some(id), not_initialized_error());
                            future::ok(Some(Outgoing::Response(res)))
                        }
                    }),
                    (other, _) => Box::pin(match other.id().cloned() {
                        None => future::ok(None),
                        Some(id) => {
                            let res = Response::error(Some(id), Error::invalid_request());
                            future::ok(Some(Outgoing::Response(res)))
                        }
                    }),
                }
            }
        }
    }
}
