use std::sync::{Condvar, Mutex};

/// Bound concurrent decodes and their estimated working memory. Large files
/// run alone; waiting callers do not allocate decoded pixels.
pub(crate) struct DecodePool {
    state: Mutex<(usize, u64)>,
    changed: Condvar,
    jobs: usize,
    budget: u64,
}
pub(crate) struct Permit<'a> {
    pool: &'a DecodePool,
    cost: u64,
}
impl DecodePool {
    pub(crate) fn new(jobs: usize, budget: u64) -> Self {
        Self {
            state: Mutex::new((0, 0)),
            changed: Condvar::new(),
            jobs,
            budget,
        }
    }
    pub(crate) fn acquire(&self, cost: u64) -> Result<Permit<'_>, String> {
        let cost = cost.clamp(1, self.budget);
        let mut state = self.state.lock().map_err(|e| e.to_string())?;
        while state.0 >= self.jobs || state.1 + cost > self.budget {
            state = self.changed.wait(state).map_err(|e| e.to_string())?;
        }
        state.0 += 1;
        state.1 += cost;
        Ok(Permit { pool: self, cost })
    }
}
impl Drop for Permit<'_> {
    fn drop(&mut self) {
        let mut state = self.pool.state.lock().unwrap_or_else(|e| e.into_inner());
        state.0 -= 1;
        state.1 -= self.cost;
        self.pool.changed.notify_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::Duration;
    #[test]
    fn oversized_work_runs_alone_and_releases_waiters() {
        let pool = DecodePool::new(4, 100);
        let permit = pool.acquire(1000).unwrap();
        std::thread::scope(|scope| {
            let (tx, rx) = mpsc::channel();
            let pool = &pool;
            scope.spawn(move || {
                let _p = pool.acquire(1).unwrap();
                tx.send(()).unwrap();
            });
            assert!(rx.recv_timeout(Duration::from_millis(30)).is_err());
            drop(permit);
            rx.recv_timeout(Duration::from_secs(5)).unwrap();
        });
    }
}
