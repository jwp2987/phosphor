use std::{pin, task, time::Duration};

use futures_lite::{ready, Stream};
use instant::Instant;
use pin::Pin;
use task::{Context, Poll};
use warpui::r#async::Timer;

pub struct Throttle<S> {
    period: Duration,
    stream: S,
    timer: Option<Timer>,
}

pub fn throttle<S: Stream>(period: Duration, stream: S) -> impl Stream<Item = S::Item> {
    Throttle {
        period,
        stream,
        timer: None,
    }
}

impl<S: Stream> Stream for Throttle<S> {
    type Item = S::Item;

    fn poll_next(mut self: Pin<&mut Self>, ctx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if let Some(timer) = self.as_mut().timer() {
            ready!(timer.poll_next(ctx));

            let mut stream = self.as_mut().stream();
            let mut last_poll = Poll::Pending;
            while let Poll::Ready(item) = stream.as_mut().poll_next(ctx) {
                if item.is_none() {
                    // Stream has been closed.
                    self.set_timer(None);
                    return Poll::Ready(None);
                }
                last_poll = Poll::Ready(item)
            }

            if last_poll.is_ready() {
                let timer = Timer::after(self.period);
                self.set_timer(Some(timer));
            } else {
                self.set_timer(None);
            }
            last_poll
        } else {
            let item = ready!(self.as_mut().stream().poll_next(ctx));
            let timer = Timer::after(self.period);
            self.set_timer(Some(timer));
            Poll::Ready(item)
        }
    }
}

impl<S> Throttle<S> {
    fn timer(self: Pin<&mut Self>) -> Option<Pin<&mut Timer>> {
        if self.timer.is_some() {
            Some(unsafe { self.map_unchecked_mut(|s| s.timer.as_mut().unwrap()) })
        } else {
            None
        }
    }

    fn stream(self: Pin<&mut Self>) -> Pin<&mut S> {
        unsafe { self.map_unchecked_mut(|s| &mut s.stream) }
    }

    fn set_timer(self: Pin<&mut Self>, timer: Option<Timer>) {
        unsafe {
            self.get_unchecked_mut().timer = timer;
        }
    }
}

/// Why a [`throttle_with_bypass`] item may skip the trailing-edge wait.
pub trait Urgent {
    fn is_urgent(&self) -> bool;
}

/// Decides whether an urgent item may be delivered right now instead of waiting for the
/// throttle's timer. Kept free of any clock or timer so it can be tested with injected instants.
#[derive(Debug, Clone, Copy)]
pub struct BypassGate {
    /// Smallest gap between two deliveries for which an urgent item may skip the wait. Keeps
    /// a flood of urgent items from out-running the throttle.
    floor: Duration,
    last_delivery: Option<Instant>,
}

impl BypassGate {
    pub fn new(floor: Duration) -> Self {
        Self {
            floor,
            last_delivery: None,
        }
    }

    pub fn record_delivery(&mut self, at: Instant) {
        self.last_delivery = Some(at);
    }

    /// True when an item that arrived at `now` should be delivered immediately.
    #[cfg(test)]
    pub fn may_bypass(&self, now: Instant, urgent: bool) -> bool {
        self.bypass_delay(now, urgent) == Some(Duration::ZERO)
    }

    /// How long an item that arrived at `now` may skip ahead of the throttle: `None` for an item
    /// that is not urgent (it waits for the period), zero for an urgent item past the floor, and
    /// otherwise the rest of the floor -- an urgent item inside the floor is delivered when the
    /// floor ends, never held to the period's trailing edge.
    pub fn bypass_delay(&self, now: Instant, urgent: bool) -> Option<Duration> {
        if !urgent {
            return None;
        }
        let since_delivery = self
            .last_delivery
            .map_or(self.floor, |at| now.saturating_duration_since(at));
        Some(self.floor.saturating_sub(since_delivery))
    }
}

/// Like [`throttle`], but an item that reports itself urgent is delivered as soon as it
/// arrives, provided the previous delivery was at least `floor` ago. Used so the echo of a
/// keystroke is not held behind the trailing edge of the wakeup throttle (#1138). Non-urgent
/// items behave exactly as with [`throttle`].
pub fn throttle_with_bypass<S>(
    period: Duration,
    floor: Duration,
    stream: S,
) -> impl Stream<Item = S::Item>
where
    S: Stream,
    S::Item: Urgent,
{
    BypassThrottle {
        period,
        stream,
        timer: None,
        pending: None,
        timer_is_floor: false,
        closed: false,
        gate: BypassGate::new(floor),
    }
}

struct BypassThrottle<S: Stream> {
    period: Duration,
    stream: S,
    timer: Option<Timer>,
    /// Latest item received while the timer was running; delivered when it fires.
    pending: Option<S::Item>,
    /// Whether `timer` was shortened to the floor for an urgent `pending` item; a later urgent
    /// item inside the same floor must not push that deadline out again.
    timer_is_floor: bool,
    closed: bool,
    gate: BypassGate,
}

impl<S: Stream> Stream for BypassThrottle<S>
where
    S::Item: Urgent,
{
    type Item = S::Item;

    fn poll_next(self: Pin<&mut Self>, ctx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        // SAFETY: `stream` is the only structurally pinned field and is never moved out of
        // `this`; every other field is only read or replaced in place.
        let this = unsafe { self.get_unchecked_mut() };

        // Drain whatever is ready. The stream is polled even while the timer runs so that
        // its waker is registered and an urgent item can be seen the moment it arrives.
        while !this.closed {
            let stream = unsafe { Pin::new_unchecked(&mut this.stream) };
            match stream.poll_next(ctx) {
                Poll::Ready(Some(item)) => {
                    let now = Instant::now();
                    let bypass_delay = this.gate.bypass_delay(now, item.is_urgent());
                    if this.timer.is_none() || bypass_delay == Some(Duration::ZERO) {
                        // Leading edge, or an urgent item past the floor: deliver now.
                        this.pending = None;
                        this.gate.record_delivery(now);
                        this.timer = Some(Timer::after(this.period));
                        this.timer_is_floor = false;
                        return Poll::Ready(Some(item));
                    }
                    if let Some(rest_of_floor) = bypass_delay
                        && !this.timer_is_floor
                    {
                        // An urgent item inside the floor: wait out the floor, not the period.
                        this.timer = Some(Timer::after(rest_of_floor));
                        this.timer_is_floor = true;
                    }
                    this.pending = Some(item);
                }
                Poll::Ready(None) => this.closed = true,
                Poll::Pending => break,
            }
        }

        if let Some(timer) = this.timer.as_mut() {
            if Pin::new(timer).poll_next(ctx).is_pending() {
                return Poll::Pending;
            }
            if let Some(item) = this.pending.take() {
                this.gate.record_delivery(Instant::now());
                this.timer = Some(Timer::after(this.period));
                this.timer_is_floor = false;
                return Poll::Ready(Some(item));
            }
            this.timer = None;
            this.timer_is_floor = false;
        }

        if this.closed {
            Poll::Ready(None)
        } else {
            Poll::Pending
        }
    }
}

#[cfg(test)]
#[path = "throttle_tests.rs"]
mod tests;
