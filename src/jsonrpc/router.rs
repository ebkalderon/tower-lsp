use std::collections::HashMap;
use std::convert::Infallible;
use std::future::Future;
use std::ops::Deref;
use std::rc::Rc;
use std::task::{Context, Poll};

use async_trait::async_trait;
use futures::future::{self, FutureExt, LocalBoxFuture};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::Value;
use tower_service::Service;

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

type MethodHandler<Recv> = Box<dyn Fn(Recv, Request) -> LocalBoxFuture<'static, Option<Response>>>;

pub struct Router<S> {
    server: Rc<AsyncRefCell<S>>,
    methods: HashMap<&'static str, MethodHandler<Rc<AsyncRefCell<S>>>>,
}

impl<S: 'static> Router<S> {
    pub fn new(server: S) -> Self {
        Router {
            server: Rc::new(AsyncRefCell::new(server)),
            methods: HashMap::new(),
        }
    }

    pub fn method<Recv, Arg, Out, F>(&mut self, name: &'static str, callback: F) -> &mut Self
    where
        Arg: FromParams,
        Out: IntoResponse,
        F: Method<Recv, Arg, Out, Receiver = S> + Clone + 'static,
    {
        self.methods.entry(name).or_insert_with(|| {
            Box::new(move |server, request| {
                let (_, id, params) = request.into_parts();

                match id {
                    Some(_) if Out::IS_NOTIFICATION => {
                        return future::ready(().into_response(id)).boxed_local()
                    }
                    None if !Out::IS_NOTIFICATION => return future::ready(None).boxed_local(),
                    _ => {}
                }

                let params = match Arg::from_params(params) {
                    Ok(params) => params,
                    Err(err) => {
                        return future::ready(id.map(|id| Response::from_error(id, err)))
                            .boxed_local()
                    }
                };

                let server = Shared(server);
                let callback = callback.clone();
                async move { callback.call(server, params).await.into_response(id) }.boxed_local()
            })
        });

        self
    }
}

/// An opaque newtype for the shared/synchcronized [`Method`] receiver.
pub struct Shared<Recv: ?Sized>(Rc<AsyncRefCell<Recv>>);

/// A trait implemented by all valid JSON-RPC method handlers.
///
/// This trait abstracts over the following classes of functions and/or closures:
///
/// ```ignore
/// // Request without parameters
/// async fn f(&self) -> jsonrpc::Result<R>;
/// async fn f(&mut self) -> jsonrpc::Result<R>;
///
/// // Request with required parameters, where `P: DeserializeOwned`, `R: Serialize + 'static`
/// async fn f(&self, params: P) -> jsonrpc::Result<R>;
/// async fn f(&mut self, params: P) -> jsonrpc::Result<R>;
///
/// // Notification without paramters
/// async fn f(&self);
/// async fn f(&mut self);
///
/// // Notification with required parameters, where `P: DeserializeOwned`
/// async fn f(&self, params: P);
/// async fn f(&mut self, params: P);
/// ```
pub trait Method<Recv, Arg, Out>: private::Sealed {
    type Receiver;
    type Future: Future<Output = Out>;

    fn call(self, recv: Shared<Self::Receiver>, arg: Arg) -> Self::Future;
}

impl<'a, Recv, Arg, Out, F> Method<&'a Recv, Arg, Out> for F
where
    Arg: FromParams,
    Out: IntoResponse,
    for<'b> F: Closure<&'b Recv, Arg, Out> + 'a,
{
    type Receiver = Recv;
    type Future = LocalBoxFuture<'a, Out>;

    fn call(self, recv: Shared<Self::Receiver>, arg: Arg) -> Self::Future {
        async move {
            let s = recv.0.read().await;
            self.invoke(&s, arg).await
        }
        .boxed_local()
    }
}

impl<'a, Recv, Arg, Out, F> Method<&'a mut Recv, Arg, Out> for F
where
    Arg: FromParams,
    Out: IntoResponse,
    for<'b> F: Closure<&'b mut Recv, Arg, Out> + 'a,
{
    type Receiver = Recv;
    type Future = LocalBoxFuture<'a, Out>;

    fn call(self, recv: Shared<Self::Receiver>, arg: Arg) -> Self::Future {
        async move {
            let mut s = recv.0.write().await;
            self.invoke(&mut s, arg).await
        }
        .boxed_local()
    }
}

/// A trait implemented by all async methods with 1 or 2 arguments.
trait Closure<R, I, O> {
    /// The future return value.
    type Future: Future<Output = O>;

    /// Invokes the method with the given receiver and argument.
    fn invoke(self, receiver: R, arg: I) -> Self::Future;
}

/// Support parameter-less JSON-RPC methods.
impl<R, O, F, Fut> Closure<R, (), O> for F
where
    F: Fn(R) -> Fut,
    Fut: Future<Output = O>,
{
    type Future = Fut;

    fn invoke(self, receiver: R, _: ()) -> Self::Future {
        self(receiver)
    }
}

/// Support JSON-RPC methods with `params`.
impl<R, I, O, F, Fut> Closure<R, (I,), O> for F
where
    F: Fn(R, I) -> Fut,
    I: DeserializeOwned,
    Fut: Future<Output = O>,
{
    type Future = Fut;

    fn invoke(self, receiver: R, arg: (I,)) -> Self::Future {
        self(receiver, arg.0)
    }
}

/// A trait implemented by all JSON-RPC method parameters.
pub trait FromParams: private::Sealed + Sized + 'static {
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
impl<P: DeserializeOwned + 'static> FromParams for (P,) {
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
pub trait IntoResponse: private::Sealed + 'static {
    const IS_NOTIFICATION: bool;

    /// Attempts to construct a [`Response`] using `Self` and a corresponding [`Id`].
    fn into_response(self, id: Option<Id>) -> Option<Response>;
}

/// Support JSON-RPC notification methods.
impl IntoResponse for () {
    const IS_NOTIFICATION: bool = true;

    fn into_response(self, id: Option<Id>) -> Option<Response> {
        id.map(|id| Response::from_error(id, Error::invalid_request()))
    }
}

/// Support JSON-RPC request methods.
impl<R: Serialize + 'static> IntoResponse for Result<R, Error> {
    const IS_NOTIFICATION: bool = false;

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
}

mod private {
    pub trait Sealed {}
    impl<T> Sealed for T {}
}
