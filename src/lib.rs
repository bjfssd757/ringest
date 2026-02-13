#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use ringest_io::Registry;
    use std::{sync::Arc, time::Duration};
    use tokio::sync::Barrier;

    fn create_test_file(path: &str) -> std::fs::File {
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            std::fs::OpenOptions::new()
                .create(true).read(true).write(true).truncate(true)
                .share_mode(7).open(path).unwrap()
        }
        #[cfg(not(windows))]
        {
            std::fs::File::create(path).unwrap()
        }
    }

    #[tokio::test]
    async fn test_consistency_full_cycle() {
        let path = format!("test_cons_{}.dat", line!());
        let registry = Arc::new(Registry::new());
        
        let file = create_test_file(&path);
        registry.insert(1, file, Duration::from_millis(1000), Duration::from_millis(1000));

        let writer = registry.get_writer::<std::fs::File>(1).unwrap();
        let reader = registry.get_reader::<std::fs::File>(1).unwrap();

        let original_data = Bytes::from("Highload consistency check");
        writer.write_at(0, original_data.clone()).await.unwrap();

        let memory_data = reader.read_at(0, original_data.len() as u64).await.unwrap();
        assert_eq!(original_data, memory_data);

        writer.flush().await.unwrap();

        let disk_data = reader.read_at(0, original_data.len() as u64).await.unwrap();
        assert_eq!(original_data, disk_data);

        drop(registry);
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn test_high_concurrency_io() {
        let path = format!("test_stress_{}.dat", line!());
        let registry = Arc::new(Registry::new());

        let file = create_test_file(&path);
        registry.insert(12345, file, Duration::from_millis(1000), Duration::from_millis(1000));

        let num_tasks = 20;
        let ops_per_task = 20;
        let barrier = Arc::new(Barrier::new(num_tasks));
        let mut handles = vec![];

        for i in 0..num_tasks {
            let reg = registry.clone();
            let bar = barrier.clone();
            let handle = tokio::spawn(async move {
                let writer = reg.get_writer::<std::fs::File>(12345).unwrap();
                let reader = reg.get_reader::<std::fs::File>(12345).unwrap();

                bar.wait().await;

                for j in 0..ops_per_task {
                    let offset = (i * ops_per_task + j) as u64 * 8;
                    let data = format!("d{:02}t{:02}xx", i, j).into_bytes();

                    writer.write_at(offset, data.clone()).await.unwrap();
                    
                    let read_data = reader.read_at(offset, 8).await.unwrap();
                    assert_eq!(read_data.as_ref(), data.as_slice());
                }
            });
            handles.push(handle);
        }

        for h in handles { h.await.unwrap(); }
        
        drop(registry);
        let _ = std::fs::remove_file(&path);
    }
}