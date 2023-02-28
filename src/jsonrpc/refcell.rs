use std::cell::{Ref, RefCell, RefMut};
use std::ops::{Deref, DerefMut};
use std::task::Poll;

use futures_intrusive::sync::LocalSemaphore;

const MAX_READERS: usize = 32;

pub struct AsyncRefCell<S: ?Sized> {
    sem: LocalSemaphore,
    inner: RefCell<S>,
}

impl<S: ?Sized> AsyncRefCell<S> {
    pub fn new(inner: S) -> Self
    where
        S: Sized,
    {
        AsyncRefCell {
            sem: LocalSemaphore::new(true, MAX_READERS),
            inner: RefCell::new(inner),
        }
    }

    pub fn is_ready(&self) -> bool {
        self.sem.permits() > 0
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

trait Releaser {}

impl<T> Releaser for T {}

pub struct ReadGuard<'a, S: ?Sized> {
    inner: Ref<'a, S>,
    _releaser: Box<dyn Releaser + 'a>,
}

impl<'a, S: ?Sized> Deref for ReadGuard<'a, S> {
    type Target = S;

    fn deref(&self) -> &Self::Target {
        &*self.inner
    }
}

pub struct WriteGuard<'a, S: ?Sized> {
    inner: RefMut<'a, S>,
    _releaser: Box<dyn Releaser + 'a>,
}

impl<'a, S: ?Sized> Deref for WriteGuard<'a, S> {
    type Target = S;

    fn deref(&self) -> &Self::Target {
        &*self.inner
    }
}

impl<'a, S: ?Sized> DerefMut for WriteGuard<'a, S> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut *self.inner
    }
}
