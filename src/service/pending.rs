//! Types for tracking cancelable client-to-server JSON-RPC requests.

use std::fmt::{self, Debug, Formatter};
use std::future::Future;
use std::sync::Arc;

use dashmap::{mapref::entry::Entry, DashMap};
use futures::future::{self, Either};
use tracing::{debug, info};

use super::ExitedError;
use crate::jsonrpc::{Error, Id, Response};

/// A hashmap containing pending server requests, keyed by request ID.
pub struct Pending(Arc<DashMap<Id, future::AbortHandle>>);

impl Pending {
    /// Creates a new pending server requests map.
    #[inline]
    pub fn new() -> Self {
        Pending(Arc::new(DashMap::new()))
    }

    /// Executes the given async request handler, keyed by the given request ID.
    ///
    /// If a cancel request is issued before the future is finished resolving, this will resolve to
    /// a "canceled" error response, and the pending request handler future will be dropped.
    pub fn execute<F>(
        &self,
        id: Id,
        fut: F,
    ) -> impl Future<Output = Result<Option<Response>, ExitedError>> + Send + 'static
    where
        F: Future<Output = Result<Option<Response>, ExitedError>> + Send + 'static,
    {
        if let Entry::Vacant(entry) = self.0.entry(id.clone()) {
            let (handler_fut, abort_handle) = future::abortable(fut);
            entry.insert(abort_handle);

            let requests = self.0.clone();
            Either::Left(async move {
                let abort_result = handler_fut.await;
                requests.remove(&id); // Remove abort handle now to avoid double cancellation.

                if let Ok(handler_result) = abort_result {
                    handler_result
                } else {
                    Ok(Some(Response::from_error(id, Error::request_cancelled())))
                }
            })
        } else {
            Either::Right(async { Ok(Some(Response::from_error(id, Error::invalid_request()))) })
        }
    }

    /// Attempts to cancel the running request handler corresponding to this ID.
    ///
    /// This will force the future to resolve to a "canceled" error response. If the future has
    /// already completed, this method call will do nothing.
    pub fn cancel(&self, id: &Id) {
        if let Some((_, handle)) = self.0.remove(id) {
            handle.abort();
            info!("successfully cancelled request with ID: {}", id);
        } else {
            debug!(
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

impl Debug for Pending {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        f.debug_set()
            .entries(self.0.iter().map(|entry| entry.key().clone()))
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[tokio::test(flavor = "current_thread")]
    async fn executes_server_request() {
        let pending = Pending::new();

        let id = Id::Number(1);
        let id2 = id.clone();
        let response = pending
            .execute(id.clone(), async {
                Ok(Some(Response::from_ok(id2, json!({}))))
            })
            .await;

        assert_eq!(response, Ok(Some(Response::from_ok(id, json!({})))));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn cancels_server_request() {
        let pending = Pending::new();

        let id = Id::Number(1);
        let handler_fut = tokio::spawn(pending.execute(id.clone(), future::pending()));

        pending.cancel(&id);

        let res = handler_fut.await.expect("task panicked");
        assert_eq!(
            res,
            Ok(Some(Response::from_error(id, Error::request_cancelled())))
        );
    }
}
