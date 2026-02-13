use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use bytes::{BufMut, Bytes, BytesMut};
use parking_lot::RwLock;
use ringest_error::{Error, Result};
use crate::{IoContext, IoTarget, IoTimeoutExt, LatencyMeasureExt, WriteQueue};

#[derive(Clone)]
pub struct PendingWrite {
    pub(crate) offset: u64,
    pub(crate) data: Bytes,
}

pub struct BufferWriter<T: IoTarget> {
    context: Arc<IoContext<T>>,
}

impl<T: IoTarget> BufferWriter<T> {
    pub fn new(context: Arc<IoContext<T>>) -> Self {
        Self {
            context,
        }
    }

    pub async fn write_at(&self, offset: u64, data: impl Into<Bytes>) -> Result<()> {
        self.context.write_at(offset, data).await
        // let bytes = data.into();
        // let avg = self.context.metrics.avg_write_latency.load(Ordering::Relaxed);

        // if avg > self.threshold_ns || bytes.len() < 4 * 1024 {
        //     let mut should_flush = false;

        //     {
        //         let mut q = self.context.write_queue.write();
        //         q.push(PendingWrite { offset, data: bytes });
        //         if q.total_bytes > 16 * 1024 {
        //             should_flush = true;
        //         }
        //     }

        //     if should_flush {
        //         self.flush().await?;
        //     }
        // } else {
        //     self.context.target.write_at(bytes, offset)
        //         .with_timeout(self.context.write_timeout)
        //         .measure_latency(&self.context.metrics.avg_write_latency)
        //         .await?;
        // }
        // Ok(())
    }

    pub async fn flush(&self) -> Result<()> {
        self.context.flush().await
        // let mut q: WriteQueue;

        // {
        //     let mut lock = self.context.write_queue.write();
        //     q = std::mem::take(&mut *lock);
        // }

        // if q.is_empty() {
        //     return Ok(())
        // }

        // q.writes.sort_by_key(|op| op.offset);

        // let mut it = q.writes.into_iter().peekable();
        // let mut combined_buffer = BytesMut::with_capacity(q.total_bytes as usize);

        // while let Some(current) = it.next() {
        //     combined_buffer.clear();
        //     combined_buffer.put(&current.data[..]);

        //     let start_offset = current.offset;

        //     while let Some(next) = it.peek() {
        //         if start_offset + combined_buffer.len() as u64 == next.offset {
        //             combined_buffer.put(&next.data[..]);
        //             it.next();
        //         } else {
        //             break;
        //         }
        //     }
        //     self.context.target.write_at(combined_buffer.split().freeze(), start_offset).await?;
        // }

        // Ok(())
    }

    pub async fn shutdown(&self) -> Result<()> {
        self.context.flush().await
    }
}

impl<T: IoTarget> Drop for BufferWriter<T> {
    fn drop(&mut self) {
        let ctx = Arc::clone(&self.context);

        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move {
                let _ = ctx.flush().await;
            });
        } else {
            std::thread::spawn(move || {
                if let Ok(rt) = tokio::runtime::Runtime::new() {
                    rt.block_on(async { let _ = ctx.flush().await; });
                }
            });
        }
    }
}