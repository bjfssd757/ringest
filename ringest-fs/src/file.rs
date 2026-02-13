use bytes::Bytes;
use ringest_io::{BufferReader, BufferWriter};
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt, BufReader, BufWriter, SeekFrom};
use std::{fs::Metadata, hash::{DefaultHasher, Hash, Hasher}, sync::Arc, time::{Duration, SystemTime}};
use crate::IO_REGISTRY;
use ringest_error::{Error, FileSystemError, Result};

#[cfg(unix)]
use std::os::unix::fs::FileExt;

#[cfg(windows)]
use std::os::windows::fs::FileExt;

#[cfg(feature = "regex")]
use regex::Regex;
use tokio::fs::DirEntry;

pub struct File {
    pub name: String,
    pub path: String,
    pub last_edit: SystemTime,
    pub created_at: SystemTime,
    pub accessed_at: SystemTime,
    pub extension: String,
    writer: BufferWriter<tokio::fs::File>,
    reader: BufferReader<tokio::fs::File>,
    metadata: Metadata,
}

impl File {
    pub async fn new(path: &str, content: String) -> Result<Self> {
        let extension = extension(&path.to_string()).unwrap_or("UNKNOWN".to_string());        
        let abs_path = std::path::Path::new(path).canonicalize().unwrap_or(path.into());

        let mut hasher = DefaultHasher::new();
        abs_path.hash(&mut hasher);
        let file_id = hasher.finish();

        let mut file = tokio::fs::File::options()
            .create(true)
            .read(true)
            .write(true)
            .truncate(true)
            .open(path)
            .await?;

        let metadata = file.metadata().await?;

        tokio::io::AsyncWriteExt::write_all(&mut file, content.as_bytes()).await;
        
        IO_REGISTRY.insert(file_id, file, Duration::from_millis(1000), Duration::from_millis(1000));

        let writer = IO_REGISTRY.get_writer::<tokio::fs::File>(file_id)
            .ok_or(Error::Internal("Failed to get writer".to_string()))?;
        let reader = IO_REGISTRY.get_reader::<tokio::fs::File>(file_id)
            .ok_or(Error::Internal("Failed to get reader".to_string()))?;

        Ok(Self {
            name: name(&path.to_string())?,
            extension,
            path: path.to_string(),
            last_edit: SystemTime::now(),
            created_at: SystemTime::now(),
            accessed_at: SystemTime::now(),
            writer,
            reader,
            metadata,
        })
    }

    pub async fn open(path: &str) -> Result<Self> {
        let file = tokio::fs::File::options()
            .read(true)
            .write(true)
            .open(path)
            .await?;
        let meta = file.metadata().await?;
        let name = name(&path.to_string()).unwrap_or("UNKNOWN".to_string());
        let extension = extension(&path.to_string()).unwrap_or("UNKNOWN".to_string());
        let last_edit = meta.modified()?;
        let accessed_at = meta.accessed()?;
        let created_at = meta.created()?;

        let mut hasher = DefaultHasher::new();
        path.hash(&mut hasher);
        let file_id = hasher.finish();

        IO_REGISTRY.insert(file_id, file, Duration::from_millis(1000), Duration::from_millis(1000));

        let writer = IO_REGISTRY.get_writer::<tokio::fs::File>(file_id)
            .ok_or(Error::Internal("Failed to get writer".to_string()))?;
        let reader = IO_REGISTRY.get_reader::<tokio::fs::File>(file_id)
            .ok_or(Error::Internal("Failed to get reader".to_string()))?;

        Ok(Self {
            name,
            extension,
            path: path.to_string(),
            last_edit,
            created_at,
            accessed_at,
            reader,
            writer,
            metadata: meta,
        })
    }

    pub async fn from_entry(entry: &DirEntry, meta: Metadata) -> Result<Self> {
        let path = entry.path().to_string_lossy().to_string();
        let ext = extension(&path).unwrap_or("UNKNOWN".to_string());
        let file = tokio::fs::File::options()
            .read(true)
            .write(true)
            .open(&path)
            .await?;

        let mut hasher = DefaultHasher::new();
        path.hash(&mut hasher);
        let file_id = hasher.finish();

        IO_REGISTRY.insert(file_id, file, Duration::from_millis(1000), Duration::from_millis(1000));

        let writer = IO_REGISTRY.get_writer::<tokio::fs::File>(file_id)
            .ok_or(Error::Internal("Failed to get writer".to_string()))?;
        let reader = IO_REGISTRY.get_reader::<tokio::fs::File>(file_id)
            .ok_or(Error::Internal("Failed to get reader".to_string()))?;

        Ok(Self {
            name: entry.file_name().to_string_lossy().to_string(),
            path,
            last_edit: meta.modified().unwrap_or(SystemTime::now()),
            created_at: meta.created().unwrap_or(SystemTime::now()),
            accessed_at: meta.accessed().unwrap_or(SystemTime::now()),
            extension: ext,
            reader,
            writer,
            metadata: meta,
        })
    }

    pub async fn can_write(&self) -> bool {
        self.metadata.permissions().readonly()
    }

    pub async fn rewrite(&self, content: String) -> Result<()> {
        self.writer.write_at(0, Bytes::from(content)).await?;
        Ok(())
    }

    pub async fn write_at(&self, offset: u64, content: String) -> Result<()> {
        self.writer.write_at(offset, Bytes::from(content)).await?;
        Ok(())
    }

    pub async fn append(&mut self, content: String) -> Result<()> {
        self.writer.write_at(self.size(), Bytes::from(content)).await?;
        Ok(())
    }

    pub async fn content(&mut self) -> Result<String> {
        let bytes = self.reader.read_at(0, self.size()).await?;
        match String::from_utf8(bytes.to_vec()) {
            Ok(content) => Ok(content),
            Err(e) => Err(Error::FileSystemError(FileSystemError::InvalidUtf8(e))),
        }
    }

    #[cfg(feature = "regex")]
    pub async fn contains_r(&mut self, re: Regex) -> Result<()> {
        let content = self.content().await?;
        
        if re.is_match(&content) {
            return Ok(())
        }
        Err(Error::FileSystemError(FileSystemError::SearchError(content.into())))
    }

    pub async fn contains(&mut self, content: &String) -> Result<()> {
        let body = self.content().await?;

        if body.contains(content) {
            return Ok(())
        }
        Err(Error::FileSystemError(FileSystemError::SearchError(content.into())))
    }

    pub async fn find(&mut self, content: &String) -> Result<usize> {
        let body = self.content().await?;

        if let Some(pos) = body.find(content) {
            return Ok(pos)
        }
        Err(Error::FileSystemError(FileSystemError::SearchError(content.into())))
    }

    pub fn size(&self) -> u64 {
        self.metadata.len()
    }

    pub async fn size_bits(&self) -> u64 {
        self.size() * 8
    }

    pub async fn size_kb(&self) -> u64 {
        self.size() / 1024
    }

    pub async fn size_mb(&self) -> u64 {
        self.size() / u64::pow(1024, 2)
    }

    pub async fn size_gb(&self) -> u64 {
        self.size() / u64::pow(1024, 3)
    }
}

fn name(path: &String) -> Result<String> {
    if let Some(pos) = path.rfind("/") {
        if let Some(pos_ext) = path.rfind(".") {
            return Ok(path[pos..pos_ext].to_string())
        }
        return Ok(path[pos..].to_string())
    } else if let Some(pos) = path.rfind("\\") {
        if let Some(pos_ext) = path.rfind(".") {
            return Ok(path[pos..pos_ext].to_string())
        }
        return Ok(path[pos..].to_string())
    } else {
        Err(Error::Internal("Failed to get name from path".to_string()))
    }
}

fn extension(path: &String) -> Result<String> {
    if let Some(pos) = path.rfind(".") {
        return Ok(path[pos..].to_string())
    }
    Err(Error::Internal("Failed to get extension from path".to_string()))
}