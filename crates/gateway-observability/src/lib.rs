//! 可观测性层目前只放最小运行时统计。
//! 这里先把接口位置占住，后面再逐步扩展日志、指标和 tracing。

use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug, Default)]
pub struct RuntimeStats {
    total_requests: AtomicU64,
}

impl RuntimeStats {
    /// 请求计数先做成无锁原子累加，满足第一阶段的基础统计需求。
    pub fn record_request(&self) {
        self.total_requests.fetch_add(1, Ordering::Relaxed);
    }

    /// 读取统计值只用于观测，不参与强一致业务判断，所以用 relaxed 即可。
    pub fn total_requests(&self) -> u64 {
        self.total_requests.load(Ordering::Relaxed)
    }
}
