use std::collections::HashMap;
use std::future::Future;
use std::rc::Rc;

use async_trait::async_trait;
use futures::future::{self, FutureExt, LocalBoxFuture};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::Value;

use super::refcell::AsyncRefCell;
use super::{Error, ErrorCode, Id, Request, Response};

fn foo() {
    struct Blah;
    impl Blah {
        async fn read_only(&self) {}
        async fn mutating(&mut self) {}
    }

    let r = Router::new(Blah)
        .method("read_only", Blah::read_only)
        .method("mutating", Blah::mutating);
}

type MethodHandler<P, R> = Box<dyn Fn(P) -> LocalBoxFuture<'static, R>>;

pub struct Router<S> {
    server: Rc<AsyncRefCell<S>>,
    methods: HashMap<&'static str, MethodHandler<Request, Option<Response>>>,
}

impl<S: 'static> Router<S> {
    pub fn new(server: S) -> Self {
        Router {
            server: Rc::new(AsyncRefCell::new(server)),
            methods: HashMap::new(),
        }
    }

    pub fn method<T, P, R, F>(&mut self, name: &'static str, callback: F) -> &mut Self
    where
        T: Receiver<S, P, R, F>,
        P: FromParams,
        R: IntoResponse,
        F: Method<T, P, R> + Clone + 'static,
    {
        let server = &self.server;
        self.methods
            .entry(name)
            .or_insert_with(|| Box::new(method_handler(server.clone(), callback)));

        self
    }

    // pub fn method<P, R, F>(&mut self, name: &'static str, callback: F) -> &mut Self
    // where
    //     P: FromParams,
    //     R: IntoResponse,
    //     F: for<'a> Method<&'a S, P, R> + Clone + 'static,
    // {
    //     let server = &self.server;
    //     self.methods
    //         .entry(name)
    //         .or_insert_with(|| Box::new(method_handler(server.clone(), callback)));
    //
    //     self
    // }
    // //
    // pub fn method_mut<P, R, F>(&mut self, name: &'static str, callback: F) -> &mut Self
    // where
    //     P: FromParams,
    //     R: IntoResponse,
    //     F: for<'a> Method<&'a mut S, P, R> + Clone + 'static,
    // {
    //     let server = &self.server;
    //     self.methods
    //         .entry(name)
    //         .or_insert_with(|| Box::new(method_handler_mut(server.clone(), callback)));
    //
    //     self
    // }
}

// fn method_handler<S, P, R, F>(
//     server: Rc<AsyncRefCell<S>>,
//     callback: F,
// ) -> MethodHandler<Request, Option<Response>>
// where
//     S: 'static,
//     P: FromParams,
//     R: IntoResponse,
//     F: for<'a> Method<&'a S, P, R> + Clone + 'static,
// {
//     Box::new(move |request| {
//         let (_, id, params) = request.into_parts();
//
//         match id {
//             Some(_) if R::is_notification() => {
//                 return future::ready(().into_response(id)).boxed_local()
//             }
//             None if !R::is_notification() => return future::ready(None).boxed_local(),
//             _ => {}
//         }
//
//         let params = match P::from_params(params) {
//             Ok(params) => params,
//             Err(err) => {
//                 return future::ready(id.map(|id| Response::from_error(id, err))).boxed_local()
//             }
//         };
//
//         let server = server.clone();
//         let callback = callback.clone();
//         async move {
//             let server = server.read().await;
//             callback.invoke(&*server, params).await.into_response(id)
//         }
//         .boxed_local()
//     })
// }
//
// fn method_handler_mut<S, P, R, F>(
//     server: Rc<AsyncRefCell<S>>,
//     callback: F,
// ) -> MethodHandler<Request, Option<Response>>
// where
//     S: 'static,
//     P: FromParams,
//     R: IntoResponse,
//     F: for<'a> Method<&'a mut S, P, R> + Clone + 'static,
// {
//     Box::new(move |request| {
//         let (_, id, params) = request.into_parts();
//
//         match id {
//             Some(_) if R::is_notification() => {
//                 return future::ready(().into_response(id)).boxed_local()
//             }
//             None if !R::is_notification() => return future::ready(None).boxed_local(),
//             _ => {}
//         }
//
//         let params = match P::from_params(params) {
//             Ok(params) => params,
//             Err(err) => {
//                 return future::ready(id.map(|id| Response::from_error(id, err))).boxed_local()
//             }
//         };
//
//         let server = server.clone();
//         let callback = callback.clone();
//         async move {
//             let mut server = server.write().await;
//             callback.invoke(&mut server, params).await.into_response(id)
//         }
//         .boxed_local()
//     })
// }

fn method_handler<T, S, P, R, F>(
    server: Rc<AsyncRefCell<S>>,
    callback: F,
) -> MethodHandler<Request, Option<Response>>
where
    T: Receiver<S, P, R, F>,
    S: 'static,
    P: FromParams,
    R: IntoResponse,
    F: Method<T, P, R> + Clone + 'static,
{
    Box::new(move |request| {
        let (_, id, params) = request.into_parts();

        match id {
            Some(_) if R::is_notification() => {
                return future::ready(().into_response(id)).boxed_local()
            }
            None if !R::is_notification() => return future::ready(None).boxed_local(),
            _ => {}
        }

        let params = match P::from_params(params) {
            Ok(params) => params,
            Err(err) => {
                return future::ready(id.map(|id| Response::from_error(id, err))).boxed_local()
            }
        };

        let server = server.clone();
        let callback = callback.clone();
        T::invoke(server, callback, id, params).boxed_local()
    })
}

#[async_trait(?Send)]
pub trait Receiver<S, P, R, F>: Sized {
    async fn invoke(
        server: Rc<AsyncRefCell<S>>,
        f: F,
        id: Option<Id>,
        params: P,
    ) -> Option<Response>;
}

#[async_trait(?Send)]
impl<'a, S, P, R, F> Receiver<S, P, R, F> for &'a S
where
    S: 'static,
    P: FromParams,
    R: IntoResponse,
    F: for<'b> Method<&'b S, P, R> + 'static,
{
    async fn invoke(
        server: Rc<AsyncRefCell<S>>,
        f: F,
        id: Option<Id>,
        params: P,
    ) -> Option<Response> {
        let server = server.read().await;
        f.invoke(&*server, params).await.into_response(id)
    }
}

#[async_trait(?Send)]
impl<'a, S, P, R, F> Receiver<S, P, R, F> for &'a mut S
where
    S: 'static,
    P: FromParams,
    R: IntoResponse,
    F: for<'b> Method<&'b mut S, P, R> + 'static,
{
    async fn invoke(
        server: Rc<AsyncRefCell<S>>,
        f: F,
        id: Option<Id>,
        params: P,
    ) -> Option<Response> {
        let mut server = server.write().await;
        f.invoke(&mut server, params).await.into_response(id)
    }
}

/// A trait implemented by all valid JSON-RPC method handlers.
///
/// This trait abstracts over the following classes of functions and/or closures:
///
/// Signature                                            | Description
/// -----------------------------------------------------|---------------------------------
/// `async fn f(&self) -> jsonrpc::Result<R>`            | Request without parameters
/// `async fn f(&self, params: P) -> jsonrpc::Result<R>` | Request with required parameters
/// `async fn f(&self)`                                  | Notification without parameters
/// `async fn f(&self, params: P)`                       | Notification with parameters
pub trait Method<S, P, R>: private::Sealed {
    /// The future response value.
    type Future: Future<Output = R>;

    /// Invokes the method with the given `server` receiver and parameters.
    fn invoke(&self, server: S, params: P) -> Self::Future;
}

/// Support parameter-less JSON-RPC methods.
impl<F, S, R, Fut> Method<S, (), R> for F
where
    F: Fn(S) -> Fut,
    Fut: Future<Output = R>,
{
    type Future = Fut;

    #[inline]
    fn invoke(&self, server: S, _: ()) -> Self::Future {
        self(server)
    }
}

/// Support JSON-RPC methods with `params`.
impl<F, S, P, R, Fut> Method<S, (P,), R> for F
where
    F: Fn(S, P) -> Fut,
    P: DeserializeOwned,
    Fut: Future<Output = R>,
{
    type Future = Fut;

    #[inline]
    fn invoke(&self, server: S, params: (P,)) -> Self::Future {
        self(server, params.0)
    }
}

/// A trait implemented by all JSON-RPC method parameters.
pub trait FromParams: private::Sealed + Send + Sized + 'static {
    /// Attempts to deserialize `Self` from the `params` value extracted from [`Request`].
    fn from_params(params: Option<Value>) -> super::Result<Self>;
}

/// Deserialize non-existent JSON-RPC parameters.
impl FromParams for () {
    fn from_params(params: Option<Value>) -> super::Result<Self> {
        if let Some(p) = params {
            Err(Error::invalid_params(format!("Unexpected params: {p}")))
        } else {
            Ok(())
        }
    }
}

/// Deserialize required JSON-RPC parameters.
impl<P: DeserializeOwned + Send + 'static> FromParams for (P,) {
    fn from_params(params: Option<Value>) -> super::Result<Self> {
        if let Some(p) = params {
            serde_json::from_value(p)
                .map(|params| (params,))
                .map_err(|e| Error::invalid_params(e.to_string()))
        } else {
            Err(Error::invalid_params("Missing params field"))
        }
    }
}

/// A trait implemented by all JSON-RPC response types.
pub trait IntoResponse: private::Sealed + Send + 'static {
    /// Attempts to construct a [`Response`] using `Self` and a corresponding [`Id`].
    fn into_response(self, id: Option<Id>) -> Option<Response>;

    /// Returns `true` if this is a notification response type.
    fn is_notification() -> bool;
}

/// Support JSON-RPC notification methods.
impl IntoResponse for () {
    fn into_response(self, id: Option<Id>) -> Option<Response> {
        id.map(|id| Response::from_error(id, Error::invalid_request()))
    }

    #[inline]
    fn is_notification() -> bool {
        true
    }
}

/// Support JSON-RPC request methods.
impl<R: Serialize + Send + 'static> IntoResponse for Result<R, Error> {
    fn into_response(self, id: Option<Id>) -> Option<Response> {
        debug_assert!(id.is_some(), "Requests always contain an `id` field");
        if let Some(id) = id {
            let result = self.and_then(|r| {
                serde_json::to_value(r).map_err(|e| Error {
                    code: ErrorCode::InternalError,
                    message: e.to_string(),
                    data: None,
                })
            });
            Some(Response::from_parts(id, result))
        } else {
            None
        }
    }

    #[inline]
    fn is_notification() -> bool {
        false
    }
}

mod private {
    pub trait Sealed {}
    impl<T> Sealed for T {}
}
