use std::{sync::{Arc, atomic::{AtomicBool, Ordering}}, time::Duration};
use bytes::{BufMut, Bytes, BytesMut};
use parking_lot::RwLock;
use tokio::sync::{Mutex, Notify};
use crate::{IoMetrics, IoTarget, IoTimeoutExt, LatencyMeasureExt, PendingRead, PendingWrite, TIME_CACHE, WriteQueue, time::TimeCache};
use ringest_error::Result;

pub struct IoContext<T: IoTarget> {
    pub target: Arc<T>,
    pub metrics: Arc<IoMetrics>,
    pub write_queue: Arc<RwLock<WriteQueue>>,
    pub read_queue: Arc<RwLock<Vec<PendingRead>>>,
    pub flushing_queue: Arc<RwLock<WriteQueue>>,
    pub write_timeout: Duration,
    pub read_timeout: Duration,
    pub threshold_ns: u64,
    pub flush_lock: Arc<Mutex<()>>,
}

impl<T: IoTarget> IoContext<T> {
    pub async fn flush(&self) -> Result<()> {
        let _guard = self.flush_lock.lock().await;

        let (mut q, total_bytes) = {
            let mut w_lock = self.write_queue.write();
            if w_lock.is_empty() { return Ok(()); }
            
            let data = std::mem::take(&mut *w_lock);
            let bytes = data.total_bytes;
            
            let mut f_lock = self.flushing_queue.write();
            *f_lock = data.clone();
            
            (data, bytes)
        };

        q.writes.sort_by_key(|op| op.offset);
        let mut it = q.writes.into_iter().peekable();
        let mut combined_buffer = BytesMut::with_capacity(total_bytes as usize);

        while let Some(current) = it.next() {
            combined_buffer.clear();
            combined_buffer.put(&current.data[..]);
            let start_offset = current.offset;

            while let Some(next) = it.peek() {
                if start_offset + combined_buffer.len() as u64 == next.offset {
                    combined_buffer.put(&next.data[..]);
                    it.next();
                } else { break; }
            }
            self.target.write_at(combined_buffer.split().freeze(), start_offset).await?;
        }

        self.flushing_queue.write().clear();
        self.metrics.last_out.store(TIME_CACHE.get_cached(), Ordering::Relaxed);
        Ok(())
    }

    pub async fn write_at(&self, offset: u64, data: impl Into<Bytes>) -> Result<()> {
        let bytes = data.into();
        let avg = self.metrics.avg_write_latency.load(Ordering::Relaxed);
        self.metrics.last_in.store(TIME_CACHE.get_cached(), Ordering::Relaxed);

        if avg > self.threshold_ns || bytes.len() < 4 * 1024 {
            let mut should_flush = false;

            {
                let mut q = self.write_queue.write();
                q.push(PendingWrite { offset, data: bytes });
                if q.total_bytes > 16 * 1024 {
                    should_flush = true;
                }
            }

            if should_flush {
                self.flush().await?;
            }
        } else {
            self.target.write_at(bytes, offset)
                .with_timeout(self.write_timeout)
                .measure_latency(&self.metrics.avg_write_latency)
                .await?;
        }
        Ok(())
    }

    pub async fn read_at(self: Arc<Self>, offset: u64, len: u64) -> Result<Bytes> {
        let read_end = offset + len;

        let find_exact_in_q = |q: &WriteQueue| {
            q.writes.iter().rev().find(|p| {
                p.offset == offset && p.data.len() as u64 == len
            }).map(|p| p.data.clone())
        };

        {
            let w_guard = self.write_queue.read();
            if let Some(data) = find_exact_in_q(&w_guard) { return Ok(data); }
            
            let f_guard = self.flushing_queue.read();
            if let Some(data) = find_exact_in_q(&f_guard) { return Ok(data); }
        }

        let mut potential_patches = Vec::new();
        let collect_patches = |q: &WriteQueue, target: &mut Vec<PendingWrite>| {
            for p in &q.writes {
                let p_end = p.offset + p.data.len() as u64;
                if p.offset < read_end && p_end > offset {
                    target.push(p.clone());
                }
            }
        };
        {
            let f_guard = self.flushing_queue.read();
            collect_patches(&f_guard, &mut potential_patches);
            
            let w_guard = self.write_queue.read();
            collect_patches(&w_guard, &mut potential_patches);
        }

        let _guard = self.flush_lock.lock().await;

        potential_patches.clear();
        {
            let f_guard = self.flushing_queue.read();
            collect_patches(&f_guard, &mut potential_patches);
            
            let w_guard = self.write_queue.read();
            collect_patches(&w_guard, &mut potential_patches);
        }

        let disk_data = self.target.read_at(offset, len as usize)
            .with_timeout(self.read_timeout)
            .measure_latency(&self.metrics.avg_read_latency)
            .await?;

        {
             let w_guard = self.write_queue.read();
             collect_patches(&w_guard, &mut potential_patches);
        }

        let mut buf = BytesMut::from(&disk_data[..]);
        
        for patch in potential_patches {
            let p_start = patch.offset;
            let p_end = patch.offset + patch.data.len() as u64;
            if p_start >= read_end || p_end <= offset { continue; }

            let start_in_buf = if p_start > offset { (p_start - offset) as usize } else { 0 };
            let end_in_buf = if p_end < read_end { (p_end - offset) as usize } else { (read_end - offset) as usize };
            
            let start_in_patch = if p_start < offset { (offset - p_start) as usize } else { 0 };
            
            let len_to_copy = end_in_buf - start_in_buf;
             
            buf[start_in_buf..end_in_buf].copy_from_slice(
                &patch.data[start_in_patch..start_in_patch + len_to_copy]
            );
        }

        Ok(buf.freeze())
    }
}