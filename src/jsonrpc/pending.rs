//! Hashmaps for tracking pending JSON-RPC requests.

use std::fmt::{self, Debug, Formatter};
use std::future::Future;
use std::sync::Arc;

use dashmap::{mapref::entry::Entry, DashMap};
use futures::channel::oneshot;
use futures::future::{self, Either};
use log::{info, warn};
use serde::Serialize;

use super::{Error, Id, Response, Result};

/// A hashmap containing pending server requests, keyed by request ID.
pub struct ServerRequests(Arc<DashMap<Id, future::AbortHandle>>);

impl ServerRequests {
    /// Creates a new pending server requests map.
    #[inline]
    pub fn new() -> Self {
        ServerRequests(Arc::new(DashMap::new()))
    }

    /// Executes the given async request handler, keyed by the given request ID.
    ///
    /// If a cancel request is issued before the future is finished resolving, this will resolve to
    /// a "canceled" error response, and the pending request handler future will be dropped.
    pub fn execute<F, T>(&self, id: Id, fut: F) -> impl Future<Output = Response> + Send + 'static
    where
        F: Future<Output = Result<T>> + Send + 'static,
        T: Serialize,
    {
        if let Entry::Vacant(entry) = self.0.entry(id.clone()) {
            let (handler_fut, abort_handle) = future::abortable(fut);
            entry.insert(abort_handle);

            let requests = self.0.clone();
            Either::Left(async move {
                let abort_result = handler_fut.await;
                requests.remove(&id); // Remove abort handle now to avoid double cancellation.

                if let Ok(handler_result) = abort_result {
                    let result = handler_result.map(|v| serde_json::to_value(v).unwrap());
                    Response::from_parts(id, result)
                } else {
                    Response::error(Some(id), Error::request_cancelled())
                }
            })
        } else {
            Either::Right(async { Response::error(Some(id), Error::invalid_request()) })
        }
    }

    /// Attempts to cancel the running request handler corresponding to this ID.
    ///
    /// This will force the future to resolve to a "canceled" error response. If the future has
    /// already completed, this method call will do nothing.
    #[inline]
    pub fn cancel(&self, id: &Id) {
        if let Some((_, handle)) = self.0.remove(id) {
            handle.abort();
            info!("successfully cancelled request with ID: {}", id);
        } else {
            warn!(
                "client asked to cancel request {}, but no such pending request exists, ignoring",
                id
            );
        }
    }

    /// Cancels all pending request handlers, if any.
    #[inline]
    pub fn cancel_all(&self) {
        self.0.retain(|_, handle| {
            handle.abort();
            false
        });
    }
}

impl Debug for ServerRequests {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        f.debug_set()
            .entries(self.0.iter().map(|entry| entry.key().clone()))
            .finish()
    }
}

/// A hashmap containing pending client requests, keyed by request ID.
pub struct ClientRequests(DashMap<Id, oneshot::Sender<Response>>);

impl ClientRequests {
    /// Creates a new pending client requests map.
    #[inline]
    pub fn new() -> Self {
        ClientRequests(DashMap::new())
    }

    /// Inserts the given response into the map.
    ///
    /// The corresponding `.wait()` future will then resolve to the given value.
    #[inline]
    pub fn insert(&self, r: Response) {
        match r.id() {
            None => warn!("received response with request ID of `null`, ignoring"),
            Some(id) => match self.0.remove(id) {
                Some((_, tx)) => tx.send(r).expect("receiver already dropped"),
                None => warn!("received response with unknown request ID: {}", id),
            },
        }
    }

    /// Marks the given request ID as pending and waits for its corresponding response to arrive.
    ///
    /// # Panics
    ///
    /// Panics if the request ID is already in the hashmap and is pending a matching response. This
    /// should never happen provided that a monotonically increasing `id` value is used.
    #[inline]
    pub fn wait(&self, id: Id) -> impl Future<Output = Response> + Send + 'static {
        match self.0.entry(id) {
            Entry::Vacant(entry) => {
                let (tx, rx) = oneshot::channel();
                entry.insert(tx);
                async { rx.await.expect("sender already dropped") }
            }
            _ => panic!("concurrent waits for the same request ID can't happen, this is a bug"),
        }
    }
}

impl Debug for ClientRequests {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        f.debug_set()
            .entries(self.0.iter().map(|entry| entry.key().clone()))
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use serde_json::json;

    use super::*;

    #[tokio::test]
    async fn executes_server_request() {
        let pending = ServerRequests::new();

        let id = Id::Number(1);
        let response = pending.execute(id.clone(), async { Ok(json!({})) }).await;

        assert_eq!(response, Response::ok(id, json!({})));
    }

    #[tokio::test]
    async fn cancels_server_request() {
        let pending = ServerRequests::new();

        let id = Id::Number(1);
        let handler_fut = tokio::spawn(pending.execute(id.clone(), async {
            tokio::time::sleep(Duration::from_secs(50)).await;
            Ok(json!({}))
        }));

        tokio::time::sleep(Duration::from_millis(30)).await;
        pending.cancel(&id);

        let res = handler_fut.await.expect("task panicked");
        assert_eq!(res, Response::error(Some(id), Error::request_cancelled()));
    }

    #[tokio::test]
    async fn waits_for_client_response() {
        let pending = ClientRequests::new();

        let id = Id::Number(1);
        let wait_fut = tokio::spawn(pending.wait(id.clone()));

        let expected = Response::ok(id.clone(), json!({}));
        pending.insert(expected.clone());

        let actual = wait_fut.await.expect("task panicked");
        assert_eq!(expected, actual);
    }
}
