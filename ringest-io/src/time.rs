use std::{sync::{Arc, atomic::{AtomicU64, Ordering}}, time::{Duration, SystemTime, UNIX_EPOCH}};

use tokio::time;

pub struct TimeCache {
    current_ms: Arc<AtomicU64>,
}

impl TimeCache {
    pub fn new(tick_interval: Duration) -> Self {
        let current_ms = Arc::new(AtomicU64::new(Self::now_sys()));
        let current_ms_clone = Arc::clone(&current_ms);

        tokio::spawn(async move {
            let mut interval = time::interval(tick_interval);
            interval.set_missed_tick_behavior(time::MissedTickBehavior::Skip);

            loop {
                interval.tick().await;
                current_ms_clone.store(Self::now_sys(), Ordering::Relaxed);
            }
        });

        Self {
            current_ms
        }
    }

    fn now_sys() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards!")
            .as_millis() as u64
    }

    pub fn get_cached(&self) -> u64 {
        self.current_ms.load(Ordering::Relaxed)
    }
}