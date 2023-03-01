#![allow(missing_debug_implementations)]
#![allow(missing_docs)]

use std::cell::{Ref, RefCell, RefMut};
use std::ops::{Deref, DerefMut};

use async_weighted_semaphore::{Semaphore, SemaphoreGuard};

/// Maximum number of permits to grant before subsequent acquire attempts will yield.
const MAX_PERMITS: usize = 32;

/// An async-aware `RefCell<T>`.
///
/// This synchronization primitive is similar to `tokio::sync::RwLock` but is single-threaded and
/// is not tied to any async runtime.
pub struct AsyncRefCell<T: ?Sized> {
    sem: Semaphore,
    inner: RefCell<T>,
}

impl<T> AsyncRefCell<T> {
    /// Creates a new `AsyncRefCell` containing `value`.
    pub fn new(value: T) -> Self {
        AsyncRefCell {
            sem: Semaphore::new(MAX_PERMITS),
            inner: RefCell::new(value),
        }
    }

    /// Acquires one permit, allowing up to `MAX_PERMITS` concurrent readers.
    ///
    /// The calling task will yield until there are no writers which hold the `RefCell`. There may
    /// be other readers inside the lock when the task resumes.
    ///
    /// Returns an RAII guard which will drop this read access of the `AsyncRefCell` when dropped.
    pub async fn read(&self) -> ReadGuard<'_, T> {
        ReadGuard {
            _guard: self.sem.acquire(1).await.unwrap(),
            inner: self.inner.borrow(),
        }
    }

    /// Acquires all the permits, preventing any concurrent writers or readers.
    ///
    /// The calling task will yield while other writers or readers currently have access to the
    /// `RefCell`. The first-in-first-out priority policy prevents writer starvation.
    ///
    /// Returns an RAII guard which will drop the write access of this `AsyncRefCell` when dropped.
    pub async fn write(&self) -> WriteGuard<'_, T> {
        WriteGuard {
            _guard: self.sem.acquire(MAX_PERMITS).await.unwrap(),
            inner: self.inner.borrow_mut(),
        }
    }

    /// Returns an immutable reference to the inner value.
    ///
    /// # Panics
    ///
    /// Panics if any `WriteGuard`s are currently live.
    pub fn inner(&self) -> Ref<'_, T> {
        self.inner.borrow()
    }

    /// Returns a mutable reference to the inner value.
    ///
    /// # Panics
    ///
    /// Panics if any `ReadGuard`s are currently live.
    pub fn inner_mut(&self) -> RefMut<'_, T> {
        self.inner.borrow_mut()
    }
}

/// A guard that automatically releases shared read access of an `AsyncRefCell` when dropped.
pub struct ReadGuard<'a, T: ?Sized> {
    inner: Ref<'a, T>,
    _guard: SemaphoreGuard<'a>,
}

impl<'a, T> Deref for ReadGuard<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &*self.inner
    }
}

/// A guard that automatically releases exclusive access of an `AsyncRefCell` when dropped.
pub struct WriteGuard<'a, T: ?Sized> {
    inner: RefMut<'a, T>,
    _guard: SemaphoreGuard<'a>,
}

impl<'a, T: ?Sized> Deref for WriteGuard<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &*self.inner
    }
}

impl<'a, T: ?Sized> DerefMut for WriteGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut *self.inner
    }
}
