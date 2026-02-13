use futures::{FutureExt, future::BoxFuture};
use tokio::{fs::{self, DirEntry}, task::JoinSet};
use std::{sync::Arc, path::PathBuf};

use dashmap::DashMap;
use ringest_error::{Error, FileSystemError, Result};
use crate::{IO_REGISTRY, REGISTERED_FILES, file::File, filter::{FileType, Filter}};

pub struct DirStats {
    pub total_size: u64,
    pub file_count: u64,
    pub dir_count: u64,
}

pub struct Directory {
    pub path: String,
    pub subdirectories: Arc<DashMap<String, Arc<Directory>>>,
    pub subfiles: Arc<DashMap<String, Arc<File>>>,
}

impl Directory {
    pub async fn open(path: PathBuf, filter: Arc<Filter>) -> Result<Arc<Self>> {
        let name = path.file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "/".to_string());

        let root = Arc::new(Self {
            path: name,
            subdirectories: Arc::new(DashMap::new()),
            subfiles: Arc::new(DashMap::new()),
        });

        Self::scan(
            path, 
            Arc::clone(&filter), 
            root.subdirectories.clone(),
            root.subfiles.clone(),
            0
        ).await;

        Ok(root)
    }

    pub fn stats(&self) -> DirStats {
        let mut stats = DirStats {
            total_size: 0,
            file_count: self.subfiles.len() as u64,
            dir_count: self.subdirectories.len() as u64,
        };

        for file in self.subfiles.iter() {
            stats.total_size += file.size();
        }

        for dir in self.subdirectories.iter() {
            let sub_stats = dir.stats();
            stats.total_size += sub_stats.total_size;
            stats.file_count += sub_stats.file_count;
            stats.dir_count += sub_stats.dir_count;
        }

        stats
    }

    pub async fn remove(&self, name: &str) -> Result<()> {
        if let Some((_, file_arc)) = self.subfiles.remove(name) {
            tokio::fs::remove_file(&file_arc.path).await?;
            let id = {
                let entry = REGISTERED_FILES
                    .iter()
                    .find(|f| f.0 == name && f.1 == self.path)
                    .ok_or_else(|| Error::FileSystemError(FileSystemError::PathNotFound(PathBuf::from(name))))?;

                *entry.key() 
            };
            let _ = IO_REGISTRY.remove(id);

            return Ok(())
        }

        if let Some((_, dir_arc)) = self.subdirectories.remove(name) {
            tokio::fs::remove_dir_all(&dir_arc.path).await?;
            return Ok(())
        }

        Err(Error::FileSystemError(FileSystemError::PathNotFound(PathBuf::from(name))))
    }

    pub async fn move_to_trash(&self, name: &str) -> Result<()> {
        let path = if let Some((_, file)) = self.subfiles.remove(name) {
            file.writer.flush().await?;
            
            PathBuf::from(&file.path)
        } else if let Some((_, dir)) = self.subdirectories.remove(name) {
            PathBuf::from(&dir.path)
        } else {
            return Err(Error::FileSystemError(FileSystemError::PathNotFound(PathBuf::from(name))))
        };

        tokio::task::spawn_blocking(move || {
            trash::delete(&path)
        })
        .await
        .map_err(|e| Error::Internal(e.to_string()))?
        .map_err(|e| Error::Internal(e.to_string()))?;

        Ok(())
    }

    pub fn find<P>(&self, predicate: &P) -> Option<FileType>
    where
        P: Fn(&FileType) -> bool,
    {
        for entry in self.subfiles.iter() {
            let mode = FileType::File(Arc::clone(entry.value()));
            if predicate(&mode) {
                return Some(mode);
            }
        }

        for entry in self.subdirectories.iter() {
            let subdir = entry.value();
            let mode = FileType::Dir(Arc::clone(subdir));

            if predicate(&mode) {
                return Some(mode)
            }

            if let Some(found) = subdir.find(predicate) {
                return Some(found)
            }
        }

        None
    }

    pub fn find_all<P>(&self, predicate: &P, results: &mut Vec<FileType>)
    where
        P: Fn(&FileType) -> bool
    {
        for entry in self.subfiles.iter() {
            let mode = FileType::File(Arc::clone(entry.value()));
            if predicate(&mode) {
                results.push(mode);
            }
        }

        for entry in self.subdirectories.iter() {
            let subdir = entry.value();
            let node = FileType::Dir(Arc::clone(subdir));
            if predicate(&node) {
                results.push(node);
            }
            subdir.find_all(predicate, results);
        }
    }

    pub fn scan(
        path: PathBuf, 
        filter: Arc<Filter>, 
        res_d: Arc<DashMap<String, Arc<Directory>>>, 
        res_f: Arc<DashMap<String, Arc<File>>>, 
        depth: u64
    ) -> BoxFuture<'static, ()> {
        async move {
            if let Some(max_depth) = filter.recursive_depth {
                if depth > max_depth { return; }
            }

            let mut entries = match tokio::fs::read_dir(&path).await {
                Ok(e) => e,
                Err(_) => return,
            };

            let mut set = JoinSet::new();

            while let Ok(Some(entry)) = entries.next_entry().await {
                let file_name = entry.file_name().to_string_lossy().to_string();

                if let Some(excludes) = &filter.exclude_dirs {
                    if excludes.contains(&file_name) { continue; }
                }

                let metadata = match entry.metadata().await {
                    Ok(m) => m,
                    Err(_) => continue,
                };

                if !filter.allows(&entry, &metadata) {
                    continue;
                }

                if metadata.is_dir() && filter.recursive {
                    let new_dir = Arc::new(Directory {
                        path: file_name.clone(),
                        subdirectories: Arc::new(DashMap::new()),
                        subfiles: Arc::new(DashMap::new()),
                    });

                    res_d.insert(file_name, Arc::clone(&new_dir));

                    let filter_clone = Arc::clone(&filter);
                    let entry_path = entry.path();

                    set.spawn(Self::scan(
                        entry_path, 
                        filter_clone, 
                        new_dir.subdirectories.clone(),
                        new_dir.subfiles.clone(),
                        depth + 1
                    ));
                } else {
                    if let Ok(file) = File::from_entry(&entry, metadata) {
                        res_f.insert(file_name, Arc::new(file));
                    }
                }
            }

            while let Some(_) = set.join_next().await {}
        }.boxed()
    }
}