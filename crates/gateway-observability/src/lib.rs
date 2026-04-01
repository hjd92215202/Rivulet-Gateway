use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug, Default)]
pub struct RuntimeStats {
    total_requests: AtomicU64,
}

impl RuntimeStats {
    pub fn record_request(&self) {
        self.total_requests.fetch_add(1, Ordering::Relaxed);
    }

    pub fn total_requests(&self) -> u64 {
        self.total_requests.load(Ordering::Relaxed)
    }
}
