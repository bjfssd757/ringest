use std::{io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write}, time::SystemTime};
use crate::error::{Error, ErrorKind, SearchErrorKind};

#[cfg(feature = "regex")]
use regex::Regex;

pub struct File {
    pub name: String,
    pub path: String,
    pub last_edit: SystemTime,
    pub created_at: SystemTime,
    pub extension: String,
    writer: BufWriter<std::fs::File>,
    reader: BufReader<std::fs::File>,
}

impl File {
    pub fn new(name: &str, path: &str, content: String) -> Result<Self, Error> {
        let extension = extension(&name.to_string()).unwrap_or("UNKNOWN".to_string());
        let mut file = std::fs::File::create(name)?;
        file.write_all(content.as_bytes())?;
        file.flush()?;
        let writer = BufWriter::new(file.try_clone()?);
        let reader = BufReader::new(file);
        Ok(Self {
            name: name.to_string(),
            extension,
            path: path.to_string(),
            last_edit: SystemTime::now(),
            created_at: SystemTime::now(),
            writer,
            reader,
        })
    }

    pub fn open(path: &str) -> Result<Self, Error> {
        let file = std::fs::File::open(path)?;
        let writer = BufWriter::new(file.try_clone()?);
        let reader = BufReader::new(file.try_clone()?);
        let name = name(&path.to_string()).unwrap_or("UNKNOWN".to_string());
        let extension = extension(&path.to_string()).unwrap_or("UNKNOWN".to_string());
        let last_edit = file.metadata().map(|m| m.modified())??;
        let created_at = file.metadata().map(|m| m.created())??;
        Ok(Self {
            name,
            extension,
            path: path.to_string(),
            last_edit,
            created_at,
            writer,
            reader,
        })
    }

    pub fn can_write(&self) -> bool {
        if self.reader.get_ref().metadata().map(|m| m.permissions().readonly()).unwrap_or(true) {
            return false
        }
        true
    }

    pub fn rewrite(&mut self, content: String) -> Result<(), Error> {
        self.sync()?;
        let file = self.writer.get_mut();
        file.set_len(0)?;
        file.seek(SeekFrom::Start(0))?;

        self.writer.write_all(content.as_bytes())?;
        self.writer.flush()?;
        Ok(())
    }

    pub fn append(&mut self, content: String) -> Result<(), Error> {
        self.sync()?;
        self.writer.seek(SeekFrom::End(0))?;
        self.writer.write_all(content.as_bytes())?;
        Ok(())
    }

    pub fn content(&mut self) -> Result<String, Error> {
        self.sync()?;
        let mut content = String::new();
        self.reader.get_ref().read_to_string(&mut content)?;
        Ok(content)
    }

    #[cfg(feature = "regex")]
    pub fn contains_r(&mut self, re: Regex) -> Result<(), Error> {
        use crate::error::{ErrorKind, SearchErrorKind};

        self.sync()?;
        let content = self.content()?;
        
        if re.is_match(&content) {
            return Ok(())
        }
        Err(Error::new(ErrorKind::SearchError(SearchErrorKind::NotFound)))
    }

    pub fn contains(&mut self, content: &String) -> Result<(), Error> {
        self.sync()?;

        let mut body = String::new();
        self.reader.get_ref().read_to_string(&mut body)?;

        if body.contains(content) {
            return Ok(())
        }
        Err(Error::new(ErrorKind::SearchError(SearchErrorKind::NotFound)))
    }

    pub fn find(&mut self, content: &String) -> Result<usize, Error> {
        self.sync()?;

        let mut body = String::new();
        self.reader.get_ref().read_to_string(&mut body)?;

        if let Some(pos) = body.find(content) {
            return Ok(pos)
        }
        Err(Error::new(ErrorKind::SearchError(SearchErrorKind::NotFound)))
    }

    pub fn size(&self) -> Result<u64, Error> {
        Ok(self.reader.get_ref().metadata()?.len())
    }

    pub fn size_bits(&self) -> Result<u64, Error> {
        Ok(self.size()? * 8)
    }

    pub fn size_kb(&self) -> Result<u64, Error> {
        Ok(self.size()? / 1024)
    }

    pub fn size_mb(&self) -> Result<u64, Error> {
        Ok(self.size()? / u64::pow(1024, 2))
    }

    pub fn size_gb(&self) -> Result<u64, Error> {
        Ok(self.size()? / u64::pow(1024, 3))
    }

    pub fn sync(&mut self) -> Result<(), Error> {
        self.writer.flush()?;
        self.reader.get_mut().seek(SeekFrom::Current(0))?;
        Ok(())
    }
}

impl Drop for File {
    fn drop(&mut self) {
        self.sync().expect(format!("Error on Drop file with name: {}", self.name).as_str());
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