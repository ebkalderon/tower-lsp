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
        .map(|(method, variant_name)| {
            let rpc_name = &method.rpc_name;
            let variant = match (method.result.is_some(), method.params) {
                (true, Some(p)) => quote!(#variant_name { params: Params<#p>, id: Id },),
                (true, None) => quote!(#variant_name { id: Id },),
                (false, Some(p)) => quote!(#variant_name { params: Params<#p> },),
                (false, None) => quote!(#variant_name,),
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
        .filter_map(|(method, variant_name)| match method.result {
            Some(_) => Some(quote!(ServerMethod::#variant_name { ref id, .. } => Some(id),)),
            None => None,
        })
        .collect();

    let route_match_arms: proc_macro2::TokenStream = methods
        .iter()
        .zip(variant_names.iter())
        .map(|(method, variant_name)| {
            let rpc_name = method.rpc_name.as_str();
            let handler = &method.handler_name;
            match (rpc_name, method.result.is_some(), method.params.is_some()) {
                ("initialize", true, true) => quote! {
                    ServerMethod::#variant_name { params, id } if !is_init && !is_shut_down => {
                        trace!("received server request {:?} (ID: {})", #rpc_name, id);
                        let initialized = initialized.clone();
                        Box::pin(async move {
                            let params = match params {
                                Params::Valid(p) => p,
                                Params::Invalid(_) => {
                                    let res = Response::error(Some(id), Error::invalid_params());
                                    return Ok(Some(Outgoing::Response(res)));
                                }
                            };

                            let res = match server.initialize(params).await {
                                Err(error) => Response::error(Some(id), error),
                                Ok(result) => {
                                    let result = serde_json::to_value(result).unwrap();
                                    info!("language server initialized");
                                    initialized.store(true, Ordering::SeqCst);
                                    Response::ok(id, result)
                                }
                            };

                            Ok(Some(Outgoing::Response(res)))
                        })
                    },
                    ServerMethod::#variant_name { id, .. } if !is_shut_down => {
                        trace!("received server request {:?} (ID: {})", #rpc_name, id);
                        let res = Response::error(Some(id), Error::invalid_request());
                        future::ok(Some(Outgoing::Response(res))).boxed()
                    },
                },
                ("shutdown", true, false) => quote! {
                    ServerMethod::#variant_name { id } if is_init && !is_shut_down => {
                        trace!("received server request {:?} (ID: {})", #rpc_name, id);
                        info!("shutdown request received, shutting down");
                        let shut_down = shut_down.clone();
                        shut_down.store(true, Ordering::SeqCst);
                        pending
                            .execute(id, async move { server.#handler().await })
                            .map(|v| Ok(Some(Outgoing::Response(v))))
                            .boxed()
                    },
                },
                (_, true, true) => quote! {
                    ServerMethod::#variant_name { params: Params::Valid(p), id } if is_init && !is_shut_down => {
                        trace!("received server request {:?} (ID: {})", #rpc_name, id);
                        pending
                            .execute(id, async move { server.#handler(p).await })
                            .map(|v| Ok(Some(Outgoing::Response(v))))
                            .boxed()
                    },
                    ServerMethod::#variant_name { params: Params::Invalid(_), id } if is_init && !is_shut_down => {
                        trace!("received server request {:?} (ID: {})", #rpc_name, id);
                        warn!("invalid parameters for {:?} (ID: {})", #rpc_name, id);
                        let res = Response::error(Some(id), Error::invalid_params());
                        future::ok(Some(Outgoing::Response(res))).boxed()
                    },
                },
                (_, true, false) => quote! {
                    ServerMethod::#variant_name { id } if is_init && !is_shut_down => {
                        trace!("received server request {:?} (ID: {})", #rpc_name, id);
                        pending
                            .execute(id, async move { server.#handler().await })
                            .map(|v| Ok(Some(Outgoing::Response(v))))
                            .boxed()
                    },
                },
                (_, false, true) => quote! {
                    ServerMethod::#variant_name { params: Params::Valid(p) } if is_init && !is_shut_down => {
                        trace!("received server notification {:?}", #rpc_name);
                        Box::pin(async move { server.#handler(p).await; Ok(None) })
                    },
                    ServerMethod::#variant_name { params: Params::Invalid(_) } if is_init && !is_shut_down => {
                        trace!("received server notification {:?}", #rpc_name);
                        warn!("invalid parameters for {:?}", #rpc_name);
                        future::ok(None).boxed()
                    },
                },
                (_, false, false) => quote! {
                    ServerMethod::#variant_name if is_init && !is_shut_down => {
                        trace!("received server notification {:?}", #rpc_name);
                        Box::pin(async move { server.#handler().await; Ok(None) })
                    },
                },
            }
        })
        .collect();

    quote! {
        mod generated_impl {
            use std::future::Future;
            use std::pin::Pin;
            use std::sync::atomic::{AtomicBool, Ordering};
            use std::sync::Arc;

            use futures::{future, FutureExt};
            use log::{error, info, trace, warn};
            use lsp_types::*;
            use lsp_types::request::{
                GotoDeclarationParams, GotoImplementationParams, GotoTypeDefinitionParams,
            };

            use super::#trait_name;
            use crate::jsonrpc::{
                not_initialized_error, Error, ErrorCode, Id, Outgoing, Response, ServerRequests,
                Version,
            };
            use crate::service::ExitedError;

            /// A client-to-server LSP request.
            #[derive(Clone, Debug, PartialEq, serde::Deserialize)]
            #[cfg_attr(test, derive(::serde::Serialize))]
            pub struct ServerRequest {
                jsonrpc: Version,
                #[serde(flatten)]
                inner: ServerMethod,
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
                #[inline]
                fn id(&self) -> Option<&Id> {
                    match *self {
                        #id_match_arms
                        _ => None,
                    }
                }
            }

            #[derive(Clone, Debug, PartialEq, serde::Deserialize)]
            #[cfg_attr(test, derive(serde::Serialize))]
            #[serde(untagged)]
            enum Params<T> {
                Valid(T),
                Invalid(serde_json::Value),
            }

            pub fn handle_request<T: #trait_name>(
                server: T,
                initialized: &Arc<AtomicBool>,
                shut_down: &AtomicBool,
                stopped: &AtomicBool,
                pending: &ServerRequests,
                incoming: ServerRequest,
            ) -> Pin<Box<dyn Future<Output = Result<Option<Outgoing>, ExitedError>> + Send>> {
                let is_init = initialized.load(Ordering::SeqCst);
                let is_shut_down = shut_down.load(Ordering::SeqCst);
                match incoming.inner {
                    #route_match_arms
                    ServerMethod::CancelRequest { id } if !is_shut_down => {
                        pending.cancel(&id);
                        future::ok(None).boxed()
                    },
                    ServerMethod::Exit if !is_shut_down => {
                        info!("exit notification received, stopping");
                        stopped.store(true, Ordering::SeqCst);
                        pending.cancel_all();
                        future::ok(None).boxed()
                    },
                    other if !is_shut_down => Box::pin(match other.id().cloned() {
                        None => future::ok(None),
                        Some(id) => {
                            let response = Response::error(Some(id), not_initialized_error());
                            future::ok(Some(Outgoing::Response(response)))
                        },
                    }),
                    other => Box::pin(match other.id().cloned() {
                        None => future::ok(None),
                        Some(id) => {
                            let response = Response::error(Some(id), Error::invalid_request());
                            future::ok(Some(Outgoing::Response(response)))
                        },
                    }),
                }
            }
        }
    }
}
