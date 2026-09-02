//! Thread pool with structured fork/join.
//!
//! - [`Pool::spawn`]: fire-and-forget background work.
//! - [`Pool::scope`]: spawn jobs that borrow the stack; returns when all of
//!   them are done. The calling thread helps run queued jobs while waiting,
//!   so nested scopes on a saturated pool cannot deadlock.
//! - [`Pool::parallel_for`] / [`Pool::parallel_chunks_mut`] /
//!   [`Pool::parallel_map`]: the common shapes, built on `scope`.
//!
//! Panics inside jobs are caught and re-raised from `scope` on the caller's
//! thread (fire-and-forget panics are logged). Window, input, UI and GPU
//! submission never run here — this is for data-parallel work over
//! document snapshots (D008).

use core::marker::PhantomData;
use core::ops::Range;
use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::any::Any;
use std::collections::VecDeque;
use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, OnceLock};
use std::thread::{self, JoinHandle};
use std::time::Duration;

type Job = Box<dyn FnOnce() + Send + 'static>;

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn panic_message(p: &(dyn Any + Send)) -> &str {
    p.downcast_ref::<&str>()
        .copied()
        .or_else(|| p.downcast_ref::<String>().map(String::as_str))
        .unwrap_or("<non-string panic payload>")
}

struct Shared {
    queue: Mutex<VecDeque<Job>>,
    available: Condvar,
    shutdown: AtomicBool,
}

pub struct Pool {
    shared: Arc<Shared>,
    workers: Vec<JoinHandle<()>>,
    threads: usize,
}

fn worker(shared: Arc<Shared>) {
    loop {
        let job = {
            let mut q = lock(&shared.queue);
            loop {
                if let Some(j) = q.pop_front() {
                    break Some(j);
                }
                if shared.shutdown.load(Ordering::Acquire) {
                    break None;
                }
                q = shared.available.wait(q).unwrap_or_else(|e| e.into_inner());
            }
        };
        match job {
            Some(j) => j(),
            None => return,
        }
    }
}

impl Pool {
    /// A pool with `threads` workers (at least one).
    pub fn new(threads: usize) -> Pool {
        let threads = threads.max(1);
        let shared = Arc::new(Shared {
            queue: Mutex::new(VecDeque::new()),
            available: Condvar::new(),
            shutdown: AtomicBool::new(false),
        });
        let workers = (0..threads)
            .map(|i| {
                let s = Arc::clone(&shared);
                thread::Builder::new()
                    .name(format!("prism-job-{i}"))
                    .spawn(move || worker(s))
                    .expect("failed to spawn job worker")
            })
            .collect();
        Pool { shared, workers, threads }
    }

    /// The process-wide pool, sized to the hardware thread count.
    pub fn global() -> &'static Pool {
        static POOL: OnceLock<Pool> = OnceLock::new();
        POOL.get_or_init(|| Pool::new(thread::available_parallelism().map_or(4, |n| n.get())))
    }

    #[inline]
    pub fn threads(&self) -> usize {
        self.threads
    }

    /// Jobs waiting to be picked up (for diagnostics).
    pub fn queued(&self) -> usize {
        lock(&self.shared.queue).len()
    }

    fn push(&self, job: Job) {
        lock(&self.shared.queue).push_back(job);
        self.shared.available.notify_one();
    }

    fn try_pop(&self) -> Option<Job> {
        lock(&self.shared.queue).pop_front()
    }

    /// Fire-and-forget. A panic is logged, not propagated.
    pub fn spawn<F: FnOnce() + Send + 'static>(&self, f: F) {
        self.push(Box::new(move || {
            if let Err(p) = catch_unwind(AssertUnwindSafe(f)) {
                crate::log_error!("background job panicked: {}", panic_message(&*p));
            }
        }));
    }

    /// Structured fork/join. Jobs spawned on the scope may borrow anything
    /// that outlives the call; `scope` returns only after all of them finish.
    /// If any job panicked, `scope` panics with the first payload.
    pub fn scope<'env, F, R>(&self, f: F) -> R
    where
        F: for<'scope> FnOnce(&'scope Scope<'scope, 'env>) -> R,
    {
        let scope = Scope { pool: self, data: Arc::new(ScopeData::default()), _marker: PhantomData };
        let result = catch_unwind(AssertUnwindSafe(|| f(&scope)));
        // Jobs borrow the caller's stack: they MUST finish before we unwind.
        scope.data.wait_all(self);
        match result {
            Err(p) => resume_unwind(p),
            Ok(r) => {
                if let Some(p) = lock(&scope.data.panic).take() {
                    resume_unwind(p);
                }
                r
            }
        }
    }

    /// Run `f` over sub-ranges of `range` in parallel. Sub-ranges are at
    /// least `min_grain` long (and at least one), sized so the pool has a few
    /// jobs per thread. Small ranges run inline.
    pub fn parallel_for<F>(&self, range: Range<usize>, min_grain: usize, f: F)
    where
        F: Fn(Range<usize>) + Sync,
    {
        let len = range.len();
        if len == 0 {
            return;
        }
        let grain = (len / (self.threads * 4)).max(min_grain).max(1);
        if len <= grain {
            f(range);
            return;
        }
        let f = &f;
        self.scope(|s| {
            let mut start = range.start;
            while start < range.end {
                let end = (start + grain).min(range.end);
                s.spawn(move || f(start..end));
                start = end;
            }
        });
    }

    /// `f(chunk_index, chunk)` for every `chunk_len`-sized piece of `data`.
    pub fn parallel_chunks_mut<T, F>(&self, data: &mut [T], chunk_len: usize, f: F)
    where
        T: Send,
        F: Fn(usize, &mut [T]) + Sync,
    {
        let f = &f;
        self.scope(|s| {
            for (i, c) in data.chunks_mut(chunk_len.max(1)).enumerate() {
                s.spawn(move || f(i, c));
            }
        });
    }

    /// Map in parallel, preserving order.
    pub fn parallel_map<T, R, F>(&self, items: &[T], f: F) -> Vec<R>
    where
        T: Sync,
        R: Send,
        F: Fn(&T) -> R + Sync,
    {
        let n = items.len();
        if n == 0 {
            return Vec::new();
        }
        let grain = (n / (self.threads * 4)).max(1);
        let slots: Vec<Mutex<Vec<R>>> = (0..n.div_ceil(grain)).map(|_| Mutex::new(Vec::new())).collect();
        let (f, slots_ref) = (&f, &slots);
        self.scope(|s| {
            for (ci, chunk) in items.chunks(grain).enumerate() {
                s.spawn(move || {
                    let out: Vec<R> = chunk.iter().map(f).collect();
                    *lock(&slots_ref[ci]) = out;
                });
            }
        });
        slots.into_iter().flat_map(|m| m.into_inner().unwrap_or_else(|e| e.into_inner())).collect()
    }
}

impl Drop for Pool {
    /// Finishes queued jobs, then joins the workers.
    fn drop(&mut self) {
        self.shared.shutdown.store(true, Ordering::Release);
        self.shared.available.notify_all();
        for w in self.workers.drain(..) {
            let _ = w.join();
        }
    }
}

#[derive(Default)]
struct ScopeData {
    pending: AtomicUsize,
    lock: Mutex<()>,
    done: Condvar,
    panic: Mutex<Option<Box<dyn Any + Send>>>,
}

impl ScopeData {
    fn finish_one(&self) {
        if self.pending.fetch_sub(1, Ordering::SeqCst) == 1 {
            let _g = lock(&self.lock);
            self.done.notify_all();
        }
    }

    /// Block until `pending` is zero, running queued jobs while waiting.
    fn wait_all(&self, pool: &Pool) {
        loop {
            if self.pending.load(Ordering::SeqCst) == 0 {
                return;
            }
            if let Some(job) = pool.try_pop() {
                job();
                continue;
            }
            let g = lock(&self.lock);
            if self.pending.load(Ordering::SeqCst) == 0 {
                return;
            }
            // The timeout is belt-and-braces: it lets us notice jobs pushed by
            // other threads while we slept, so we can help with those too.
            let _ = self.done.wait_timeout(g, Duration::from_millis(1));
        }
    }
}

/// Handle for spawning borrowed jobs inside [`Pool::scope`].
pub struct Scope<'scope, 'env: 'scope> {
    pool: &'scope Pool,
    data: Arc<ScopeData>,
    _marker: PhantomData<&'scope mut &'env ()>,
}

impl<'scope> Scope<'scope, '_> {
    pub fn spawn<F>(&'scope self, f: F)
    where
        F: FnOnce() + Send + 'scope,
    {
        self.data.pending.fetch_add(1, Ordering::SeqCst);
        let data = Arc::clone(&self.data);
        let boxed: Box<dyn FnOnce() + Send + 'scope> = Box::new(f);
        // SAFETY: `Pool::scope` calls `wait_all` before returning, so every
        // borrow captured by `f` (valid for `'scope`) outlives the job's
        // execution. The job never runs after `wait_all` observes zero.
        let boxed: Box<dyn FnOnce() + Send + 'static> = unsafe { core::mem::transmute(boxed) };
        self.pool.push(Box::new(move || {
            if let Err(p) = catch_unwind(AssertUnwindSafe(boxed)) {
                let mut slot = lock(&data.panic);
                if slot.is_none() {
                    *slot = Some(p);
                }
            }
            data.finish_one();
        }));
    }

    #[inline]
    pub fn pool(&self) -> &'scope Pool {
        self.pool
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::sync::atomic::AtomicU64;

    #[test]
    fn parallel_for_covers_every_index_once() {
        let pool = Pool::new(4);
        let hits: Vec<AtomicUsize> = (0..10_000).map(|_| AtomicUsize::new(0)).collect();
        pool.parallel_for(0..10_000, 16, |r| {
            for i in r {
                hits[i].fetch_add(1, Ordering::Relaxed);
            }
        });
        assert!(hits.iter().all(|h| h.load(Ordering::Relaxed) == 1));
        // Empty and tiny ranges are fine.
        pool.parallel_for(5..5, 1, |_| panic!("must not run"));
        let ran = AtomicUsize::new(0);
        pool.parallel_for(0..3, 100, |r| {
            ran.fetch_add(r.len(), Ordering::Relaxed);
        });
        assert_eq!(ran.load(Ordering::Relaxed), 3);
    }

    #[test]
    fn scope_borrows_the_stack() {
        let pool = Pool::new(3);
        let mut data = vec![0u32; 5000];
        pool.scope(|s| {
            for (i, chunk) in data.chunks_mut(100).enumerate() {
                s.spawn(move || chunk.fill(i as u32));
            }
        });
        for (i, chunk) in data.chunks(100).enumerate() {
            assert!(chunk.iter().all(|&v| v == i as u32));
        }
    }

    #[test]
    fn nested_scopes_on_a_tiny_pool_do_not_deadlock() {
        let pool = Pool::new(2);
        let total = AtomicU64::new(0);
        pool.scope(|outer| {
            for _ in 0..8 {
                let total = &total;
                let pool = outer.pool();
                outer.spawn(move || {
                    pool.scope(|inner| {
                        for _ in 0..8 {
                            inner.spawn(move || {
                                pool.parallel_for(0..100, 10, |r| {
                                    total.fetch_add(r.len() as u64, Ordering::Relaxed);
                                });
                            });
                        }
                    });
                });
            }
        });
        assert_eq!(total.load(Ordering::Relaxed), 8 * 8 * 100);
    }

    #[test]
    fn job_panic_propagates_after_all_jobs_finish() {
        let pool = Pool::new(2);
        let finished = AtomicUsize::new(0);
        let r = catch_unwind(AssertUnwindSafe(|| {
            pool.scope(|s| {
                for i in 0..10 {
                    let finished = &finished;
                    s.spawn(move || {
                        if i == 3 {
                            panic!("job three exploded");
                        }
                        finished.fetch_add(1, Ordering::Relaxed);
                    });
                }
            })
        }));
        let p = r.expect_err("scope should re-raise the job panic");
        assert_eq!(panic_message(&*p), "job three exploded");
        assert_eq!(finished.load(Ordering::Relaxed), 9, "the other jobs still completed");
        // The pool is still healthy afterwards.
        let n = AtomicUsize::new(0);
        pool.parallel_for(0..50, 1, |r| {
            n.fetch_add(r.len(), Ordering::Relaxed);
        });
        assert_eq!(n.load(Ordering::Relaxed), 50);
    }

    #[test]
    fn body_panic_still_waits_for_jobs() {
        let pool = Pool::new(2);
        let done = Arc::new(AtomicUsize::new(0));
        let r = catch_unwind(AssertUnwindSafe(|| {
            pool.scope(|s| {
                let d = Arc::clone(&done);
                s.spawn(move || {
                    thread::sleep(Duration::from_millis(20));
                    d.fetch_add(1, Ordering::SeqCst);
                });
                panic!("body panic");
            })
        }));
        assert!(r.is_err());
        assert_eq!(done.load(Ordering::SeqCst), 1, "job finished before the panic escaped");
    }

    #[test]
    fn fire_and_forget_runs_and_drop_drains() {
        let counter = Arc::new(AtomicUsize::new(0));
        {
            let pool = Pool::new(2);
            for _ in 0..100 {
                let c = Arc::clone(&counter);
                pool.spawn(move || {
                    c.fetch_add(1, Ordering::SeqCst);
                });
            }
            let c = Arc::clone(&counter);
            pool.spawn(move || {
                panic!("logged, not fatal: {}", c.load(Ordering::SeqCst));
            });
        } // drop joins workers after the queue drains
        assert_eq!(counter.load(Ordering::SeqCst), 100);
    }

    #[test]
    fn chunks_mut_and_map_keep_order() {
        let pool = Pool::new(4);
        let mut data: Vec<usize> = (0..1000).collect();
        pool.parallel_chunks_mut(&mut data, 64, |ci, chunk| {
            for v in chunk.iter_mut() {
                *v = *v * 2 + ci;
            }
        });
        assert!(data.iter().enumerate().all(|(i, &v)| v == i * 2 + i / 64));
        pool.parallel_chunks_mut(&mut data, 64, |ci, chunk| {
            for v in chunk.iter_mut() {
                *v -= ci;
            }
        });
        let squares = pool.parallel_map(&data, |&v| v * v);
        assert_eq!(squares.len(), 1000);
        assert!(squares.iter().enumerate().all(|(i, &v)| v == (i * 2) * (i * 2)));
        assert!(pool.parallel_map(&Vec::<u8>::new(), |&v| v).is_empty());
    }

    #[test]
    fn global_pool_exists() {
        let p = Pool::global();
        assert!(p.threads() >= 1);
        let n = AtomicUsize::new(0);
        p.parallel_for(0..1000, 1, |r| {
            n.fetch_add(r.len(), Ordering::Relaxed);
        });
        assert_eq!(n.load(Ordering::Relaxed), 1000);
        assert_eq!(p.queued(), 0);
    }
}
