// lib.rs
pub mod read;
pub mod write;
pub mod ctx;
pub mod time;

use bytes::{BufMut, Bytes, BytesMut};
use dashmap::DashMap;
use async_trait::async_trait;
use parking_lot::RwLock;
use ringest_error::{Result, Error};
use tokio::sync::{Mutex, Notify};
use std::sync::LazyLock;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use std::{any::Any, sync::Arc};

#[cfg(unix)]
use std::os::unix::fs::FileExt;
#[cfg(windows)]
use std::os::windows::fs::FileExt;

pub use crate::read::BufferReader;
use crate::read::PendingRead;
use crate::time::TimeCache;
pub use crate::write::BufferWriter;
use crate::write::PendingWrite;
use crate::ctx::IoContext;

pub(crate) static TIME_CACHE: LazyLock<TimeCache> = LazyLock::new(|| TimeCache::new(Duration::from_millis(5)));

#[async_trait]
pub trait IoTimeoutExt<T> {
    async fn with_timeout(self, duration: Duration) -> Result<T>;
}

#[async_trait]
impl<F, T> IoTimeoutExt<T> for F
where
    F: std::future::Future<Output = Result<T>> + Send,
{
    async fn with_timeout(self, duration: Duration) -> Result<T> {
        tokio::time::timeout(duration, self)
            .await
            .map_err(|_| Error::Timeout)?
            .map_err(Into::into)
    }
}

#[async_trait]
pub trait IoTarget: Send + Sync + 'static {
    async fn read_at(&self, offset: u64, len: usize) -> Result<Bytes>;
    async fn write_at(&self, content: Bytes, offset: u64) -> Result<()>;
}

#[derive(Default, Clone)]
pub struct WriteQueue {
    pub writes: Vec<PendingWrite>,
    pub total_bytes: u64,
}

impl WriteQueue {
    pub fn new() -> Self {
        Self {
            writes: Vec::new(),
            total_bytes: 0,
        }
    }

    pub fn push(&mut self, op: PendingWrite) {
        self.total_bytes += op.data.len() as u64;
        self.writes.push(op);
    }

    pub fn clear(&mut self) {
        self.writes.clear();
        self.total_bytes = 0;
    }

    pub fn len(&self) -> usize {
        self.writes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.writes.is_empty()
    }
}

pub struct IoMetrics {
    pub avg_write_latency: AtomicU64,
    pub avg_read_latency: AtomicU64,
    pub total_ops: AtomicU64,
    /// Last write to buffer
    pub last_in: AtomicU64,
    /// Last flush to target
    pub last_out: AtomicU64,
}

impl IoMetrics {
    pub fn new() -> Self {
        Self {
            avg_read_latency: AtomicU64::new(0),
            avg_write_latency: AtomicU64::new(0),
            total_ops: AtomicU64::new(0),
            last_in: AtomicU64::new(0),
            last_out: AtomicU64::new(0),
        }
    }
}

pub struct Registry {
    targets: DashMap<u64, Arc<dyn Any + Send + Sync>>,
}

impl Registry {
    pub fn new() -> Self {
        Self { targets: DashMap::new() }
    }

    pub fn insert<T: IoTarget>(&self, id: u64, target: T, write_timeout: Duration, read_timeout: Duration) {
        let ctx = Arc::new(IoContext {
            target: Arc::new(target),
            metrics: Arc::new(IoMetrics::new()),
            write_queue: Arc::new(RwLock::new(WriteQueue::new())),
            read_queue: Arc::new(RwLock::new(Vec::new())),
            flushing_queue: Arc::new(RwLock::new(WriteQueue::new())),
            write_timeout,
            read_timeout,
            threshold_ns: 1_000_000,
            flush_lock: Arc::new(Mutex::new(())),
        });
        self.targets.insert(id, ctx);
    }

    pub fn get_writer<T: IoTarget>(&self, id: u64) -> Option<BufferWriter<T>> {
        let ctx = self.targets.get(&id)?;
        let context = ctx.value().clone().downcast::<IoContext<T>>().ok()?;
        Some(BufferWriter::new(context))
    }

    pub fn get_reader<T: IoTarget>(&self, id: u64) -> Option<BufferReader<T>> {
        let ctx = self.targets.get(&id)?;
        let context = ctx.value().clone().downcast::<IoContext<T>>().ok()?;
        Some(BufferReader::new(context))
    }

    pub fn start_janitor<T: IoTarget>(self: Arc<Self>, threshold_ms: u64, interval: Duration) {
        tokio::spawn(async move {
            let mut timer = tokio::time::interval(interval);
            loop {
                timer.tick().await;
                let now = TIME_CACHE.get_cached();

                for entry in self.targets.iter() {
                    if let Ok(ctx) = entry.value().clone().downcast::<IoContext<T>>() {
                        let last_in = ctx.metrics.last_in.load(Ordering::Relaxed);
                        let last_out = ctx.metrics.last_out.load(Ordering::Relaxed);

                        if last_in > last_out && (now - last_in) > threshold_ms {
                            let ctx_clone = Arc::clone(&ctx);
                            tokio::spawn(async move {
                                let _ = ctx_clone.flush().await;
                            });
                        }
                    }
                }
            }
        });
    }
}

pub trait PositionalIo {
    fn read_at_pos(&self, offset: u64, len: usize) -> std::io::Result<Vec<u8>>;
    fn write_at_pos(&self, offset: u64, data: &[u8]) -> std::io::Result<()>;
}

impl PositionalIo for std::fs::File {
    fn read_at_pos(&self, offset: u64, len: usize) -> std::io::Result<Vec<u8>> {
        let mut buf = vec![0u8; len];
        #[cfg(unix)]
        FileExt::read_at(self, &mut buf, offset)?;
        #[cfg(windows)]
        FileExt::seek_read(self, &mut buf, offset)?;
        Ok(buf)
    }

    fn write_at_pos(&self, offset: u64, data: &[u8]) -> std::io::Result<()> {
        #[cfg(unix)]
        FileExt::write_at(self, data, offset)?;
        #[cfg(windows)]
        FileExt::seek_write(self, data, offset)?;
        Ok(())
    }
}

#[async_trait]
impl IoTarget for std::fs::File {
    async fn read_at(&self, offset: u64, len: usize) -> Result<Bytes> {
        let file = self.try_clone()?;

        let data = tokio::task::spawn_blocking(move || {
            file.read_at_pos(offset, len)
        }).await.map_err(|_| std::io::Error::new(std::io::ErrorKind::Other, "Join error"))??;

        Ok(Bytes::from(data))
    }

    async fn write_at(&self, content: Bytes, offset: u64) -> Result<()> {
        let file = self.try_clone()?;

        tokio::task::spawn_blocking(move || {
            file.write_at_pos(offset, &content)
        }).await.map_err(|_| std::io::Error::new(std::io::ErrorKind::Other, "Join error"))??;

        Ok(())
    }
}

#[async_trait]
impl IoTarget for tokio::fs::File {
    async fn read_at(&self, offset: u64, len: usize) -> Result<Bytes> {
        let std_file = self.try_clone().await?.into_std().await;

        let data = tokio::task::spawn_blocking(move || {
            std_file.read_at_pos(offset, len)
        }).await.map_err(|_| std::io::Error::new(std::io::ErrorKind::Other, "Join error"))??;

        Ok(Bytes::from(data))
    }

    async fn write_at(&self, content: Bytes, offset: u64) -> Result<()> {
        let std_file = self.try_clone().await?.into_std().await;

        tokio::task::spawn_blocking(move || {
            std_file.write_at_pos(offset, &content)
        }).await.map_err(|_| std::io::Error::new(std::io::ErrorKind::Other, "Join error"))??;

        Ok(())
    }
}


#[async_trait]
pub trait LatencyMeasureExt: Sized {
    type Out;

    async fn measure_latency(self, metric: &AtomicU64) -> Self::Out;
}

#[async_trait]
impl<F> LatencyMeasureExt for F
where
    F: Future + Send,
    F::Output: Send,
{
    type Out = F::Output;

    async fn measure_latency(self, metric: &AtomicU64) -> Self::Out {
        let start = minstant::Instant::now();
        let result = self.await;
        let latency = start.elapsed().as_micros() as u64;

        metric.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |avg| {
            if avg == 0 { Some(latency) }
            else { Some((avg * 9 + latency) / 10) } 
        }).ok();

        result
    }
}