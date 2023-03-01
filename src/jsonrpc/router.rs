//! Lightweight JSON-RPC router service.

#![allow(missing_docs)]

use std::cell::{Ref, RefMut};
use std::collections::HashMap;
use std::convert::Infallible;
use std::fmt::{self, Debug, Formatter};
use std::future::Future;
use std::marker::PhantomData;
use std::rc::Rc;
use std::task::{Context, Poll};

use futures::future::{self, FutureExt, LocalBoxFuture};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::Value;
use tower::{util::UnsyncBoxService, Layer, Service};

use super::refcell::AsyncRefCell;
use super::{Error, ErrorCode, Id, Request, Response};

/// A modular JSON-RPC 2.0 request router service.
pub struct Router<S, E = Infallible> {
    server: Rc<AsyncRefCell<S>>,
    methods: HashMap<&'static str, UnsyncBoxService<Request, Option<Response>, E>>,
}

impl<S: 'static, E> Router<S, E> {
    /// Creates a new `Router` with the given shared state.
    pub fn new(server: S) -> Self {
        Router {
            server: Rc::new(AsyncRefCell::new(server)),
            methods: HashMap::new(),
        }
    }

    /// Returns a reference to the inner server.
    pub fn inner(&self) -> Ref<'_, S> {
        self.server.inner()
    }

    /// Returns a mutable reference to the inner server.
    pub fn inner_mut(&mut self) -> RefMut<'_, S> {
        self.server.inner_mut()
    }

    /// Registers a new RPC method which constructs a response with the given `callback`.
    ///
    /// The `layer` argument can be used to inject middleware into the method handler, if desired.
    pub fn method<Recv, Params, Output, F, L>(
        &mut self,
        name: &'static str,
        callback: F,
        layer: L,
    ) -> &mut Self
    where
        Params: FromParams,
        Output: IntoResponse,
        F: Method<Recv, Params, Output = Output, Server = S> + Clone + 'static,
        L: Layer<MethodHandler<S, Recv, Params, Output, E, F>>,
        L::Service: Service<Request, Response = Option<Response>, Error = E> + 'static,
    {
        self.methods.entry(name).or_insert_with(|| {
            let server = self.server.clone();
            let handler = MethodHandler::new(server, callback);
            UnsyncBoxService::new(layer.layer(handler))
        });

        self
    }
}

impl<S: Debug, E> Debug for Router<S, E> {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        f.debug_struct("Router")
            .field("server", &self.server.inner())
            .field("methods", &self.methods.keys())
            .finish()
    }
}

impl<S, E: 'static> Service<Request> for Router<S, E> {
    type Response = Option<Response>;
    type Error = E;
    type Future = LocalBoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: Request) -> Self::Future {
        if let Some(handler) = self.methods.get_mut(req.method()) {
            handler.call(req)
        } else {
            let (method, id, _) = req.into_parts();
            future::ok(id.map(|id| {
                let mut error = Error::method_not_found();
                error.data = Some(Value::from(method));
                Response::from_error(id, error)
            }))
            .boxed_local()
        }
    }
}

/// Opaque JSON-RPC method handler.
pub struct MethodHandler<Server, Recv, Params, Output, Error, F> {
    server: Rc<AsyncRefCell<Server>>,
    callback: F,
    _marker: PhantomData<(Server, Recv, Params, Output, Error)>,
}

impl<S, R, P, O, E, F> MethodHandler<S, R, P, O, E, F>
where
    P: FromParams,
    O: IntoResponse,
    F: Method<R, P, Output = O, Server = S>,
{
    fn new(server: Rc<AsyncRefCell<S>>, handler: F) -> Self {
        MethodHandler {
            server,
            callback: handler,
            _marker: PhantomData,
        }
    }
}

impl<S, R, P, O, E, F> Service<Request> for MethodHandler<S, R, P, O, E, F>
where
    S: 'static,
    P: FromParams,
    O: IntoResponse,
    F: Method<R, P, Output = O, Server = S> + Clone + 'static,
{
    type Response = Option<Response>;
    type Error = E;
    type Future = LocalBoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: Request) -> Self::Future {
        let (_, id, params) = req.into_parts();

        match (&id, O::IS_NOTIFICATION) {
            (Some(_), true) => return Box::pin(async { Ok(().into_response(id)) }),
            (None, false) => return Box::pin(async { Ok(None) }),
            _ => {}
        }

        let params = match P::from_params(params) {
            Ok(params) => params,
            Err(err) => return Box::pin(async { Ok(id.map(|id| Response::from_error(id, err))) }),
        };

        let callback = self.callback.clone();
        let server = self.server.clone();
        Box::pin(async move { Ok(callback.call(server, params).await.into_response(id)) })
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
    /// Indicates whether this indicates a notification response type.
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
                    message: e.to_string().into(),
                    data: None,
                })
            });
            Some(Response::from_parts(id, result))
        } else {
            None
        }
    }
}

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
pub trait Method<Recv, Params>: private::Sealed {
    /// The method receiver, fully dereferenced.
    type Server;
    /// The return value of this method.
    type Output;
    /// The future return value.
    type Future: Future<Output = Self::Output>;

    /// Invokes the method with the given receiver and argument.
    fn call(self, recv: Rc<AsyncRefCell<Self::Server>>, params: Params) -> Self::Future;
}

impl<'a, R: 'static, P, O, F> Method<&'a R, P> for F
where
    P: FromParams,
    O: IntoResponse,
    F: for<'b> AsyncFn<&'b R, P, Output = O> + 'a,
{
    type Server = R;
    type Output = O;
    type Future = LocalBoxFuture<'a, Self::Output>;

    fn call(self, recv: Rc<AsyncRefCell<Self::Server>>, params: P) -> Self::Future {
        Box::pin(async move {
            let server = recv.read().await;
            self.call_async(&server, params).await
        })
    }
}

impl<'a, R: 'static, P, O, F> Method<&'a mut R, P> for F
where
    P: FromParams,
    O: IntoResponse,
    F: for<'b> AsyncFn<&'b mut R, P, Output = O> + 'a,
{
    type Server = R;
    type Output = O;
    type Future = LocalBoxFuture<'a, Self::Output>;

    fn call(self, recv: Rc<AsyncRefCell<Self::Server>>, params: P) -> Self::Future {
        Box::pin(async move {
            let mut server = recv.write().await;
            self.call_async(&mut server, params).await
        })
    }
}

/// A trait implemented by all async methods with 0 or 1 arguments.
trait AsyncFn<Recv, Arg>: private::Sealed {
    /// The return value of this method.
    type Output;
    /// The future return value.
    type Future: Future<Output = Self::Output>;

    /// Invokes the method with the given receiver and argument.
    fn call_async(&self, recv: Recv, arg: Arg) -> Self::Future;
}

/// Support parameter-less JSON-RPC methods.
impl<Recv, Out, F, Fut> AsyncFn<Recv, ()> for F
where
    F: Fn(Recv) -> Fut,
    Fut: Future<Output = Out>,
{
    type Output = Out;
    type Future = Fut;

    fn call_async(&self, recv: Recv, _: ()) -> Self::Future {
        self(recv)
    }
}

/// Support JSON-RPC methods with `params`.
impl<Recv, Arg, Out, F, Fut> AsyncFn<Recv, (Arg,)> for F
where
    F: Fn(Recv, Arg) -> Fut,
    Fut: Future<Output = Out>,
{
    type Output = Out;
    type Future = Fut;

    fn call_async(&self, recv: Recv, arg: (Arg,)) -> Self::Future {
        self(recv, arg.0)
    }
}

mod private {
    pub trait Sealed {}
    impl<T> Sealed for T {}
}

#[cfg(test)]
mod tests {
    use serde::{Deserialize, Serialize};
    use serde_json::json;
    use tower::layer::layer_fn;
    use tower::ServiceExt;

    use super::*;

    #[derive(Deserialize, Serialize)]
    struct Params {
        foo: i32,
        bar: String,
    }

    struct Mock;

    impl Mock {
        async fn request(&self) -> Result<Value, Error> {
            Ok(Value::Null)
        }

        async fn request_params(&self, params: Params) -> Result<Params, Error> {
            Ok(params)
        }

        async fn notification(&self) {}

        async fn notification_params(&self, _params: Params) {}
    }

    #[tokio::test(flavor = "current_thread")]
    async fn routes_requests() {
        let mut router: Router<Mock> = Router::new(Mock);
        router
            .method("first", Mock::request, layer_fn(|s| s))
            .method("second", Mock::request_params, layer_fn(|s| s));

        let request = Request::build("first").id(0).finish();
        let response = router.ready().await.unwrap().call(request).await;
        assert_eq!(response, Ok(Some(Response::from_ok(0.into(), Value::Null))));

        let params = json!({"foo": -123i32, "bar": "hello world"});
        let with_params = Request::build("second")
            .params(params.clone())
            .id(1)
            .finish();
        let response = router.ready().await.unwrap().call(with_params).await;
        assert_eq!(response, Ok(Some(Response::from_ok(1.into(), params))));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn routes_notifications() {
        let mut router: Router<Mock> = Router::new(Mock);
        router
            .method("first", Mock::notification, layer_fn(|s| s))
            .method("second", Mock::notification_params, layer_fn(|s| s));

        let request = Request::build("first").finish();
        let response = router.ready().await.unwrap().call(request).await;
        assert_eq!(response, Ok(None));

        let params = json!({"foo": -123i32, "bar": "hello world"});
        let with_params = Request::build("second").params(params).finish();
        let response = router.ready().await.unwrap().call(with_params).await;
        assert_eq!(response, Ok(None));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn rejects_request_with_invalid_params() {
        let mut router: Router<Mock> = Router::new(Mock);
        router.method("request", Mock::request_params, layer_fn(|s| s));

        let invalid_params = Request::build("request")
            .params(json!("wrong"))
            .id(0)
            .finish();

        let response = router.ready().await.unwrap().call(invalid_params).await;
        assert_eq!(
            response,
            Ok(Some(Response::from_error(
                0.into(),
                Error::invalid_params("invalid type: string \"wrong\", expected struct Params"),
            )))
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn ignores_notification_with_invalid_params() {
        let mut router: Router<Mock> = Router::new(Mock);
        router.method("notification", Mock::request_params, layer_fn(|s| s));

        let invalid_params = Request::build("notification")
            .params(json!("wrong"))
            .finish();

        let response = router.ready().await.unwrap().call(invalid_params).await;
        assert_eq!(response, Ok(None));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn handles_incorrect_request_types() {
        let mut router: Router<Mock> = Router::new(Mock);
        router
            .method("first", Mock::request, layer_fn(|s| s))
            .method("second", Mock::notification, layer_fn(|s| s));

        let request = Request::build("first").finish();
        let response = router.ready().await.unwrap().call(request).await;
        assert_eq!(response, Ok(None));

        let request = Request::build("second").id(0).finish();
        let response = router.ready().await.unwrap().call(request).await;
        assert_eq!(
            response,
            Ok(Some(Response::from_error(
                0.into(),
                Error::invalid_request(),
            )))
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn responds_to_nonexistent_request() {
        let mut router: Router<Mock> = Router::new(Mock);

        let request = Request::build("foo").id(0).finish();
        let response = router.ready().await.unwrap().call(request).await;
        let mut error = Error::method_not_found();
        error.data = Some("foo".into());
        assert_eq!(response, Ok(Some(Response::from_error(0.into(), error))));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn ignores_nonexistent_notification() {
        let mut router: Router<Mock> = Router::new(Mock);

        let request = Request::build("foo").finish();
        let response = router.ready().await.unwrap().call(request).await;
        assert_eq!(response, Ok(None));
    }
}
