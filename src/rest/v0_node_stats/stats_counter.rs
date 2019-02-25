use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

#[derive(Clone, Debug, Default)]
pub struct StatsCounter {
    stats: Arc<StatsCounterImpl>,
}

#[derive(Debug, Default)]
struct StatsCounterImpl {
    tx_recv_cnt: AtomicUsize,
    block_recv_cnt: AtomicUsize,
}

impl StatsCounter {
    pub fn add_tx_recv_cnt(&self, count: usize) {
        self.stats.tx_recv_cnt.fetch_add(count, Ordering::Relaxed);
    }

    pub fn get_tx_recv_cnt(&self) -> usize {
        self.stats.tx_recv_cnt.load(Ordering::Relaxed)
    }

    pub fn add_block_recv_cnt(&self, count: usize) {
        self.stats.block_recv_cnt.fetch_add(count, Ordering::Relaxed);
    }

    pub fn get_block_recv_cnt(&self) -> usize {
        self.stats.block_recv_cnt.load(Ordering::Relaxed)
    }
}
