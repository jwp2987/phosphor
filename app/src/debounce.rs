use std::{pin, task, time::Duration};

use futures_lite::{Stream, ready};
use pin::Pin;
use pin_project::pin_project;
use task::{Context, Poll};
use warpui::r#async::Timer;

/// Debounce takes in a stream and limits the rate of firing events from the stream
/// by bundling all events occurred within the set interval into one.
///
/// For example, if the interval is set to 50 and the following events were fired:
/// E1 fired at T_0
/// E2 fired at T_30
/// E3 fired at T_60
/// E4 fired at T_200
///
/// Debounce will first receive E1 at T_0 and set the timer to expire at T_50. At T_30,
/// E2 came in and the previous timer has not expired, debounce will change the last
/// event to E2 and set timer at T_80 (T_30 + 50). At T_60, E3 came in and similarly
/// the previous timer has not expired, debounce will change the last
/// event to E3 and set timer at T_110 (T_60 + 50). At T_110, the timer expired and emits
/// E3. At T_200, E4 came in and debounce set the timer at T_250. At T_250, E4 was emitted.
///
/// +---------+                      +-----------+    +-----------+
/// | stream  |                      | debounce  |    | executor  |
/// +---------+                      +-----------+    +-----------+
///      |                                 |                |
///      | T_0 E1                          |                |
///      |-------------------------------->|                |
///      |-------------------------------\ |                |
///      || record event and start timer |-|                |
///      ||------------------------------| |                |
///      |                                 |                |
///      | T_30 E2                         |                |
///      |-------------------------------->|                |
///      |  -----------------------------\ |                |
///      |  | timer not expired yet:     |-|                |
///      |  | reset timer and last event | |                |
///      |  |----------------------------| |                |
///      | T_60 E3                         |                |
///      |-------------------------------->|                |
///      |  -----------------------------\ |                |
///      |  | timer not expired yet:     |-|                |
///      |  | reset timer and last event | |                |
///      |  |----------------------------| |                |
///      |                                 | T_110 E3       |
///      |                                 |--------------->|
///      |    ---------------------------\ |                |
///      |    | time expired: emit event |-|                |
///      |    |--------------------------| |                |
///      |                                 |                |
///      | T_200 E4                        |                |
///      |-------------------------------->|                |
///      |    ---------------------------\ |                |
///      |    | time expired: emit event |-|                |
///      |    |--------------------------| |                |
///      |                                 |                |
///      |                                 | T_250 E4       |
///      |                                 |--------------->|
///      |                                 |                |
#[pin_project]
pub struct Debounce<S: Stream> {
    period: Duration,
    #[pin]
    stream: S,
    #[pin]
    timer: Option<Timer>,
    last_item: Option<S::Item>,
    has_expired: bool,
}

pub fn debounce<S>(period: Duration, stream: S) -> impl Stream<Item = S::Item>
where
    S: Stream,
{
    Debounce {
        period,
        stream,
        timer: None,
        last_item: None,
        has_expired: false,
    }
}

impl<S> Stream for Debounce<S>
where
    S: Stream,
{
    type Item = S::Item;

    fn poll_next(mut self: Pin<&mut Self>, ctx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut this = self.as_mut().project();

        // Stream has expired already--return Poll::Ready(None).
        if *this.has_expired {
            return Poll::Ready(None);
        }

        let mut stream = this.stream;

        // Read out everything from the stream until the stream is exhausted.
        while let Poll::Ready(item) = stream.as_mut().poll_next(ctx) {
            match item {
                Some(item) => {
                    *this.last_item = Some(item);
                    *this.timer = Some(Timer::after(*this.period));
                }
                None => {
                    *this.timer = None;
                    *this.has_expired = true;
                    return Poll::Ready(this.last_item.take());
                }
            }
        }

        if let Some(timer) = this.timer.as_pin_mut() {
            ready!(timer.poll_next(ctx));

            // The timer is done--return the last item.
            let mut this = self.project();
            *this.timer = None;
            return Poll::Ready(this.last_item.take());
        }

        Poll::Pending
    }
}

/// Like [`debounce`], but fires on the leading edge: an item that arrives while no window is
/// open is emitted immediately and opens a window of `period`. Items arriving inside the window
/// are coalesced into one trailing item, emitted when the window closes (which opens a new
/// window, so a continuous burst yields at most one item per `period`). The last item is never
/// dropped, so a consumer always ends up with the final value.
///
/// For example, with a 50 ms period, E1 at T_0, E2 at T_20 and E3 at T_30: E1 is emitted at T_0,
/// and E3 (E2 is coalesced into it) at T_50. E4 at T_200 is emitted at T_200.
pub fn debounce_leading<S>(period: Duration, stream: S) -> impl Stream<Item = S::Item>
where
    S: Stream,
{
    LeadingDebounce {
        period,
        stream,
        timer: None,
        pending: None,
        closed: false,
    }
}

#[pin_project]
struct LeadingDebounce<S: Stream> {
    period: Duration,
    #[pin]
    stream: S,
    /// Present while a window is open.
    #[pin]
    timer: Option<Timer>,
    /// The latest item received inside the open window.
    pending: Option<S::Item>,
    closed: bool,
}

impl<S> Stream for LeadingDebounce<S>
where
    S: Stream,
{
    type Item = S::Item;

    fn poll_next(mut self: Pin<&mut Self>, ctx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut this = self.as_mut().project();

        if *this.closed {
            // Flush the coalesced item, then end the stream.
            return Poll::Ready(this.pending.take());
        }

        let mut stream = this.stream;
        let mut leading = None;
        while let Poll::Ready(item) = stream.as_mut().poll_next(ctx) {
            match item {
                Some(item) => {
                    if leading.is_none() && this.timer.is_none() {
                        leading = Some(item);
                        *this.timer = Some(Timer::after(*this.period));
                    } else {
                        *this.pending = Some(item);
                    }
                }
                None => {
                    *this.closed = true;
                    break;
                }
            }
        }

        if leading.is_some() {
            return Poll::Ready(leading);
        }
        if *this.closed {
            return Poll::Ready(this.pending.take());
        }

        if let Some(timer) = this.timer.as_mut().as_pin_mut() {
            ready!(timer.poll_next(ctx));
            *this.timer = None;
            if let Some(item) = this.pending.take() {
                // The trailing emission opens the next window.
                *this.timer = Some(Timer::after(*this.period));
                return Poll::Ready(Some(item));
            }
        }

        Poll::Pending
    }
}

#[cfg(test)]
#[path = "debounce_tests.rs"]
mod tests;
