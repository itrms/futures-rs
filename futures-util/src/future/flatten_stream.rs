use core::fmt;
use core::pin::PinMut;
use futures_core::future::Future;
use futures_core::stream::Stream;
use futures_core::task::{self, Poll};

/// Future for the `flatten_stream` combinator, flattening a
/// future-of-a-stream to get just the result of the final stream as a stream.
///
/// This is created by the `Future::flatten_stream` method.
#[must_use = "streams do nothing unless polled"]
pub struct FlattenStream<Fut: Future> {
    state: State<Fut>
}

impl<Fut: Future> FlattenStream<Fut> {
    pub(super) fn new(future: Fut) -> FlattenStream<Fut> {
        FlattenStream {
            state: State::Future(future)
        }
    }
}

impl<Fut> fmt::Debug for FlattenStream<Fut>
    where Fut: Future + fmt::Debug,
          Fut::Output: fmt::Debug,
{
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("FlattenStream")
            .field("state", &self.state)
            .finish()
    }
}

#[derive(Debug)]
enum State<Fut: Future> {
    // future is not yet called or called and not ready
    Future(Fut),
    // future resolved to Stream
    Stream(Fut::Output),
}

impl<Fut> Stream for FlattenStream<Fut>
    where Fut: Future,
          Fut::Output: Stream,
{
    type Item = <Fut::Output as Stream>::Item;

    fn poll_next(mut self: PinMut<Self>, cx: &mut task::Context) -> Poll<Option<Self::Item>> {
        loop {
            // safety: data is never moved via the resulting &mut reference
            let stream = match &mut unsafe { PinMut::get_mut_unchecked(self.reborrow()) }.state {
                State::Future(f) => {
                    // safety: the future we're re-pinning here will never be moved;
                    // it will just be polled, then dropped in place
                    match unsafe { PinMut::new_unchecked(f) }.poll(cx) {
                        Poll::Pending => {
                            // State is not changed, early return.
                            return Poll::Pending
                        },
                        Poll::Ready(stream) => {
                            // Future resolved to stream.
                            // We do not return, but poll that
                            // stream in the next loop iteration.
                            stream
                        }
                    }
                }
                State::Stream(s) => {
                    // safety: the stream we're repinning here will never be moved;
                    // it will just be polled, then dropped in place
                    return unsafe { PinMut::new_unchecked(s) }.poll_next(cx);
                }
            };

            unsafe {
                // safety: we use the &mut only for an assignment, which causes
                // only an in-place drop
                PinMut::get_mut_unchecked(self.reborrow()).state = State::Stream(stream);
            }
        }
    }
}
