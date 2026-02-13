use criterion::{black_box, criterion_group, criterion_main, Criterion};
use ringest_io::{Registry, IoTarget};
use std::{sync::Arc, time::Duration};
use bytes::Bytes;

#[cfg(windows)]
use std::os::windows::fs::OpenOptionsExt;

fn open_file_windows(path: &str) -> std::fs::File {
    let mut options = std::fs::OpenOptions::new();
    options.create(true).read(true).write(true).truncate(true);
    
    #[cfg(windows)]
    {
        options.share_mode(7);
    }
    
    options.open(path).expect("Failed to open file with share mode")
}

async fn bench_read_performance(reg: Arc<Registry>, id: u64) {
    let writer = reg.get_writer::<std::fs::File>(id).unwrap();
    let reader = reg.get_reader::<std::fs::File>(id).unwrap();
    let data = Bytes::from(vec![1u8; 4096]);
    
    for i in 0..100 {
        let offset = i * 4096;
        writer.write_at(offset, data.clone()).await.unwrap();
        let _ = reader.read_at(offset, 4096).await.unwrap();
    }
}

fn read_benchmark(c: &mut Criterion) {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();

    let timestamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis();
    let path = format!("read_bench_{}.dat", timestamp);

    let registry = rt.block_on(async {
        let reg = Arc::new(Registry::new());
        let file = open_file_windows(&path);
        reg.insert(1, file, Duration::from_millis(5000), Duration::from_millis(5000));
        reg
    });

    c.bench_function("read_after_write_hot", |b| {
        b.to_async(&rt).iter(|| {
            bench_read_performance(registry.clone(), 1)
        });
    });

    drop(registry);
    let _ = std::fs::remove_file(&path);
}

criterion_group!(benches, read_benchmark);
criterion_main!(benches);