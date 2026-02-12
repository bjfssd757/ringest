use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt, BufReader, BufWriter, SeekFrom};
use std::{fs::Metadata, sync::Arc, time::SystemTime};
use crate::error::{Error, ErrorKind, SearchErrorKind};

#[cfg(unix)]
use std::os::unix::fs::FileExt;

#[cfg(windows)]
use std::os::windows::fs::FileExt;

#[cfg(feature = "regex")]
use regex::Regex;
use tokio::fs::DirEntry;

pub trait RFileExt {
    fn read_at_sync(&self, buf: &mut [u8], offset: u64) -> io::Result<usize>;
    fn write_at_sync(&self, buf: &[u8], offset: u64) -> io::Result<usize>;
}

impl RFileExt for std::fs::File {
    fn read_at_sync(&self, buf: &mut [u8], offset: u64) -> io::Result<usize> {
        #[cfg(unix)]
        return FileExt::read_at(self, buf, offset);
        #[cfg(windows)]
        return FileExt::seek_read(self, buf, offset);
    }

    fn write_at_sync(&self, buf: &[u8], offset: u64) -> io::Result<usize> {
        #[cfg(unix)]
        return FileExt::write_at(self, buf, offset);
        #[cfg(windows)]
        return FileExt::seek_write(self, buf, offset);
    }
}

pub struct File {
    pub name: String,
    pub path: String,
    pub last_edit: SystemTime,
    pub created_at: SystemTime,
    pub accessed_at: SystemTime,
    pub extension: String,
    handle: Arc<tokio::fs::File>,
    metadata: Metadata,
}

impl File {
    pub async fn new(name: &str, path: &str, content: String) -> Result<Self, Error> {
        let extension = extension(&name.to_string()).unwrap_or("UNKNOWN".to_string());
        let mut file = tokio::fs::File::options()
            .create(true)
            .read(true)
            .write(true)
            .truncate(true)
            .open(name)
            .await?;
        file.write_all(content.as_bytes()).await?;
        file.flush().await?;
        let metadata = file.metadata().await?;
        Ok(Self {
            name: name.to_string(),
            extension,
            path: path.to_string(),
            last_edit: SystemTime::now(),
            created_at: SystemTime::now(),
            accessed_at: SystemTime::now(),
            handle: Arc::new(file),
            metadata,
        })
    }

    pub async fn open(path: &str) -> Result<Self, Error> {
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
        Ok(Self {
            name,
            extension,
            path: path.to_string(),
            last_edit,
            created_at,
            accessed_at,
            handle: Arc::new(file),
            metadata: meta,
        })
    }

    pub async fn from_entry(entry: &DirEntry, meta: Metadata) -> Result<Self, Error> {
        let path = entry.path().to_string_lossy().to_string();
        let ext = extension(&path).unwrap_or("UNKNOWN".to_string());
        let file = tokio::fs::File::options()
            .read(true)
            .write(true)
            .open(&path)
            .await?;
        Ok(Self {
            name: entry.file_name().to_string_lossy().to_string(),
            path,
            last_edit: meta.modified().unwrap_or(SystemTime::now()),
            created_at: meta.created().unwrap_or(SystemTime::now()),
            accessed_at: meta.accessed().unwrap_or(SystemTime::now()),
            extension: ext,
            handle: Arc::new(file),
            metadata: meta,
        })
    }

    pub async fn close(self) -> Result<(), Error> {
        self.handle.sync_all().await?;
        Ok(())
    }

    pub async fn can_write(&self) -> bool {
        self.metadata.permissions().readonly()
    }

    pub async fn write_at(&self, content: &String, offset: u64) -> Result<usize, Error> {
        let handle = Arc::clone(&self.handle);
        let data = content.as_bytes().to_owned();
        let std_handle = &handle.into_std().await;

        tokio::task::spawn_blocking(move || {
            let mut total_written = 0;
            while total_written < data.len() {
                let n = std_handle.write_at_sync(&data[total_written..], offset + total_written as u64)?;
                if n == 0 { return Err(Error::new(ErrorKind::IoError(std::io::ErrorKind::WriteZero), "Failed to write")) }
                total_written += n;
            }
            Ok(())
        }).await.map_err(|_| Error::new(ErrorKind::Other, "Task panicked!"))?;
        Ok(0)
    }

    pub async fn rewrite(&mut self, content: String) -> Result<(), Error> {
        self.handle.set_len(0).await?;
        
        Ok(())
    }

    pub async fn append(&mut self, content: String) -> Result<(), Error> {
        self.sync().await?;
        self.writer_mut()?.seek(SeekFrom::End(0)).await?;
        self.writer_mut()?.write_all(content.as_bytes()).await?;
        Ok(())
    }

    pub async fn content(&mut self) -> Result<String, Error> {
        self.sync().await?;
        let mut content = String::new();
        self.reader_mut()?.read_to_string(&mut content).await?;
        Ok(content)
    }

    #[cfg(feature = "regex")]
    pub async fn contains_r(&mut self, re: Regex) -> Result<(), Error> {
        use crate::error::{ErrorKind, SearchErrorKind};

        self.sync().await?;
        let content = self.content().await?;
        
        if re.is_match(&content) {
            return Ok(())
        }
        Err(Error::new(ErrorKind::SearchError(SearchErrorKind::NotFound), "Given regex not found in file"))
    }

    pub async fn contains(&mut self, content: &String) -> Result<(), Error> {
        self.sync().await?;

        let mut body = String::new();
        self.reader_mut()?.read_to_string(&mut body).await?;

        if body.contains(content) {
            return Ok(())
        }
        Err(Error::new(ErrorKind::SearchError(SearchErrorKind::NotFound), "Given content not contains in file"))
    }

    pub async fn find(&mut self, content: &String) -> Result<usize, Error> {
        self.sync().await?;

        let mut body = String::new();
        self.reader_mut()?.read_to_string(&mut body).await?;

        if let Some(pos) = body.find(content) {
            return Ok(pos)
        }
        Err(Error::new(ErrorKind::SearchError(SearchErrorKind::NotFound), "Content not found!"))
    }

    pub async fn size(&self) -> Result<u64, Error> {
        Ok(self.reader_ref()?.get_ref().metadata().await?.len())
    }

    pub async fn size_bits(&self) -> Result<u64, Error> {
        Ok(self.size().await? * 8)
    }

    pub async fn size_kb(&self) -> Result<u64, Error> {
        Ok(self.size().await? / 1024)
    }

    pub async fn size_mb(&self) -> Result<u64, Error> {
        Ok(self.size().await? / u64::pow(1024, 2))
    }

    pub async fn size_gb(&self) -> Result<u64, Error> {
        Ok(self.size().await? / u64::pow(1024, 3))
    }
}

impl Drop for File {
    fn drop(&mut self) {
        if let Some(mut writer) = self.writer.take() {
            tokio::spawn(async move {
                let _ = writer.flush().await;
                let _ = writer.shutdown().await;
            });
        }
    }
}

fn name(path: &String) -> Result<String, ()> {
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
        Err(())
    }
}

fn extension(path: &String) -> Result<String, ()> {
    if let Some(pos) = path.rfind(".") {
        return Ok(path[pos..].to_string())
    }
    Err(())
}