use std::cell::{Ref, RefCell, RefMut};
use std::ops::{Deref, DerefMut};

use futures_intrusive::sync::{GenericSemaphoreAcquireFuture, LocalSemaphore};

const MAX_READERS: usize = 32;

pub struct AsyncRefCell<S> {
    inner: RefCell<S>,
    sem: LocalSemaphore,
}

impl<S> AsyncRefCell<S> {
    pub fn new(inner: S) -> Self {
        AsyncRefCell {
            inner: RefCell::new(inner),
            sem: LocalSemaphore::new(true, MAX_READERS),
        }
    }

    pub async fn read(&self) -> ReadGuard<'_, S> {
        let releaser = self.sem.acquire(1).await;
        ReadGuard {
            inner: self.inner.borrow(),
            _releaser: Box::new(releaser),
        }
    }

    pub async fn write(&self) -> WriteGuard<'_, S> {
        let releaser = self.sem.acquire(MAX_READERS).await;
        WriteGuard {
            inner: self.inner.borrow_mut(),
            _releaser: Box::new(releaser),
        }
    }
}

pub struct ReadGuard<'a, S> {
    inner: Ref<'a, S>,
    _releaser: Box<dyn Drop + 'a>,
}

impl<'a, S> Deref for ReadGuard<'a, S> {
    type Target = S;

    fn deref(&self) -> &Self::Target {
        &*self.inner
    }
}

pub struct WriteGuard<'a, S> {
    inner: RefMut<'a, S>,
    _releaser: Box<dyn Drop + 'a>,
}

impl<'a, S> Deref for WriteGuard<'a, S> {
    type Target = S;

    fn deref(&self) -> &Self::Target {
        &*self.inner
    }
}

impl<'a, S> DerefMut for WriteGuard<'a, S> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut *self.inner
    }
}
