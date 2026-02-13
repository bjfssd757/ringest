use bytes::Bytes;
use crate::{IoContext, IoTarget, IoTimeoutExt, LatencyMeasureExt, write};
use std::sync::{Arc, atomic::Ordering};
use ringest_error::Result;

pub struct PendingRead {
    offset: u64,
    len: u64,
}

pub struct BufferReader<T: IoTarget> {
    context: Arc<IoContext<T>>,
}

impl<T: IoTarget> BufferReader<T> {
    pub fn new(context: Arc<IoContext<T>>) -> Self {
        Self {
            context,
        }
    }

    pub async fn read_at(&self, offset: u64, len: u64) -> Result<Bytes> {
        Arc::clone(&self.context).read_at(offset, len).await
        // {
        //     let write_queue = self.context.write_queue.read();

        //     if !write_queue.is_empty() {
        //         if let Some(pending) = write_queue.writes.iter().find(|p| p.offset == offset && p.data.len() == len as usize) {
        //             return Ok(pending.data.clone())
        //         }
        //     }
        // }

        // let data = self.context.target.read_at(offset, len as usize)
        //     .with_timeout(self.context.read_timeout)
        //     .measure_latency(&self.context.metrics.avg_read_latency)
        //     .await?;

        // Ok(data)
    }
}