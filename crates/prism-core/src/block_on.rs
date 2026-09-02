//! Run a future to completion on the current thread. Enough for wgpu's
//! `request_adapter` / `request_device` / buffer mapping; not an executor.

use core::future::Future;
use core::pin::pin;
use core::task::{Context, Poll, Waker};
use std::task::Wake;
use std::sync::Arc;
use std::thread::{self, Thread};

struct ThreadWaker(Thread);

impl Wake for ThreadWaker {
    fn wake(self: Arc<Self>) {
        self.0.unpark();
    }
    fn wake_by_ref(self: &Arc<Self>) {
        self.0.unpark();
    }
}

pub fn block_on<F: Future>(fut: F) -> F::Output {
    let mut fut = pin!(fut);
    let waker = Waker::from(Arc::new(ThreadWaker(thread::current())));
    let mut cx = Context::from_waker(&waker);
    loop {
        match fut.as_mut().poll(&mut cx) {
            Poll::Ready(v) => return v,
            Poll::Pending => thread::park(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::sync::atomic::{AtomicBool, Ordering};
    use std::time::Duration;

    /// Resolves the second time it is polled, waking itself from another thread.
    struct Later {
        polled: bool,
    }

    impl Future for Later {
        type Output = u32;
        fn poll(mut self: core::pin::Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<u32> {
            if self.polled {
                Poll::Ready(42)
            } else {
                self.polled = true;
                let w = cx.waker().clone();
                thread::spawn(move || {
                    thread::sleep(Duration::from_millis(5));
                    w.wake();
                });
                Poll::Pending
            }
        }
    }

    #[test]
    fn ready_immediately() {
        assert_eq!(block_on(async { 1 + 1 }), 2);
    }

    #[test]
    fn wakes_after_pending() {
        assert_eq!(block_on(Later { polled: false }), 42);
    }

    #[test]
    fn spurious_unpark_is_harmless() {
        // A stray unpark before we park must not break the loop.
        thread::current().unpark();
        static RAN: AtomicBool = AtomicBool::new(false);
        block_on(async { RAN.store(true, Ordering::SeqCst) });
        assert!(RAN.load(Ordering::SeqCst));
    }
}
