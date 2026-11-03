pub use std::sync::mpsc::SendError;
use std::{
    io,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
};

use mio::{Token, Waker, event};

/// Create a [`Sender`] and [`Receiver`] pair, for sending messages into a
/// [`mio`]-managed event loop.
pub fn channel<T>() -> (Sender<T>, Receiver<T>) {
    let (tx, rx) = mpsc::channel();

    let state = Arc::new(Mutex::new(State {
        waker: None,
        needs_wake_on_register: false,
    }));

    let pending = Arc::new(AtomicUsize::new(0));

    (
        Sender {
            state: state.clone(),
            tx,
            pending: pending.clone(),
        },
        Receiver { state, rx, pending },
    )
}

/// An [`mpsc::Receiver`] wrapper.
///
/// It implements [`event::Source`] so that it can be registered with a
/// [`mio::poll::Poll`]. It ignores the [`mio::Interest`], producing readable
/// events even if read interest is not registered.
pub struct Receiver<T> {
    state: Arc<Mutex<State>>,
    rx: mpsc::Receiver<T>,
    /// Messages sent and not yet received (see [`Receiver::has_pending`]).
    pending: Arc<AtomicUsize>,
}

impl<T> Receiver<T> {
    /// Try to receive a value. It works just like [`mpsc::Receiver::try_recv`].
    pub fn try_recv(&self) -> Result<T, mpsc::TryRecvError> {
        let received = self.rx.try_recv();
        if received.is_ok() {
            self.pending.fetch_sub(1, Ordering::AcqRel);
        }
        received
    }

    /// Whether a message has been sent that [`Receiver::try_recv`] has not yet returned. Lets a
    /// loop that is busy elsewhere notice, with one atomic load, that something is waiting for it
    /// (`mpsc::Receiver` has no non-consuming peek). May report `true` for an instant before the
    /// message is visible to `try_recv`, never `false` while one is queued.
    pub fn has_pending(&self) -> bool {
        self.pending.load(Ordering::Acquire) > 0
    }
}

impl<T> event::Source for Receiver<T> {
    fn register(
        &mut self,
        registry: &mio::Registry,
        token: Token,
        _: mio::Interest,
    ) -> io::Result<()> {
        let mut state = self.state.lock().unwrap();

        if state.waker.is_none() {
            let waker = Waker::new(registry, token)?;
            if state.needs_wake_on_register {
                waker.wake()?;
                state.needs_wake_on_register = false;
            }
            state.waker = Some(waker);
        }

        Ok(())
    }

    fn reregister(
        &mut self,
        _registry: &mio::Registry,
        _token: Token,
        _: mio::Interest,
    ) -> io::Result<()> {
        // Not actually supported, so we do nothing.
        Ok(())
    }

    fn deregister(&mut self, _: &mio::Registry) -> io::Result<()> {
        // Not actually supported, so we do nothing.
        Ok(())
    }
}

/// An [`mpsc::Sender`] wrapper.
pub struct Sender<T> {
    state: Arc<Mutex<State>>,
    tx: mpsc::Sender<T>,
    pending: Arc<AtomicUsize>,
}

impl<T> Sender<T> {
    /// Try to send a value.
    ///
    /// This works the same way as [`mpsc::Sender::send`]. After sending the
    /// value, it wakes upthe [`mio::poll::Poll`].
    ///
    /// Note that I/O errors from waking up the [`mio::poll::Poll`] are
    /// swallowed.
    pub fn send(&self, t: T) -> Result<(), SendError<T>> {
        // Counted before the send so `Receiver::has_pending` never misses a queued message.
        self.pending.fetch_add(1, Ordering::AcqRel);
        if let Err(err) = self.tx.send(t) {
            self.pending.fetch_sub(1, Ordering::AcqRel);
            return Err(err);
        }

        let mut state = self.state.lock().unwrap();
        if let Some(waker) = &mut state.waker {
            let _ = waker.wake();
        } else {
            state.needs_wake_on_register = true;
        }

        Ok(())
    }
}

impl<T> Clone for Sender<T> {
    fn clone(&self) -> Self {
        Self {
            state: self.state.clone(),
            tx: self.tx.clone(),
            pending: self.pending.clone(),
        }
    }
}

struct State {
    /// The underlying waker for the channel.  This is None until the channel
    /// is registered.
    waker: Option<Waker>,
    /// Whether or not we need to wake the waker immediately upon registration.
    /// This can happen if a message was sent before the receiver was registered
    /// with a [`mio::Registry`].
    needs_wake_on_register: bool,
}

#[cfg(test)]
#[path = "mio_channel_tests.rs"]
mod tests;
