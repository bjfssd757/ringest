use tokio::{fs::{self, DirEntry}};
use std::{sync::Arc, path::PathBuf};

use dashmap::DashMap;
use ringest_error::{Error, Result};
use crate::{file::File, filter::Filter};

pub struct Directory {
    pub subdirectories: DashMap<String, Arc<DirEntry>>,
    pub subfiles: DashMap<String, Arc<File>>,
}

impl Directory {
    pub async fn open(path: &String, filter: Arc<Filter>) -> Result<Self> {
        static DEPTH: u32 = 0;

        let entries = fs::read_dir(path).await?;
        let subfiles: DashMap<String, Arc<File>> = DashMap::new();
        let subdirs: DashMap<String, Arc<DirEntry>> = DashMap::new();



        Ok(Self {
            subdirectories: DashMap::new(),
            subfiles: DashMap::new(),
        })
    }

    pub async fn scan(path: PathBuf, filter: Arc<Filter>, res_d: Arc<DashMap<String, Arc<DirEntry>>>, res_f: Arc<DashMap<String, Arc<File>>>, depth: u64) {
        if let Some(max_depth) = filter.recursive_depth {
            if depth > max_depth { return }
        }

        let mut entries = match tokio::fs::read_dir(&path).await {
            Ok(e) => e,
            Err(_) => return,
        };

        while let Ok(Some(entry)) = entries.next_entry().await {
            if let Some(excludes_dir) = &filter.exclude_dirs {
                if excludes_dir.contains(&entry.file_name().to_string_lossy().to_string()) { continue; }
            }

            let metadata = entry.metadata().await.unwrap();

            if filter.allows(&entry, &metadata) {
                continue;
            }

            if metadata.is_dir() && filter.recursive {
                let filter_clone = Arc::clone(&filter);
                let res_f_clone = Arc::clone(&res_f);
                let res_d_clone = Arc::clone(&res_d);
                let path = entry.path();
                res_d.insert(entry.file_name().to_string_lossy().into(), Arc::new(entry)).unwrap();
                tokio::spawn(async move {
                    Self::scan(path, filter_clone, res_d_clone, res_f_clone, depth + 1);
                });
            } else {
                res_f.insert(entry.file_name().to_string_lossy().into(), Arc::new(File::from_entry(&entry, metadata).await.map_err(|_| return).unwrap()));
            }
        }
    }
}