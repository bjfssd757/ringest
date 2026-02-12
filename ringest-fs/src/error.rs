use std::io::{Write, stderr};


#[derive(Debug)]
pub enum ErrorKind {
    IoError(std::io::ErrorKind),
    SearchError(SearchErrorKind),
    Other
}

#[derive(Debug)]
pub enum SearchErrorKind {
    NotFound,
}

#[derive(Debug)]
pub struct Error {
    pub kind: ErrorKind,
}

impl Error {
    pub fn new(kind: ErrorKind, message: &str) -> Self {
        let mut stderr = stderr();
        stderr.write_all(message.as_bytes()).unwrap();
        Self {
            kind,
        }
    }
}

impl std::error::Error for Error {}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[RINGEST-FS  ERROR]: {:?}", self.kind)
    }
}
impl From<std::io::Error> for Error {
    fn from(value: std::io::Error) -> Self {
        Error::new(ErrorKind::IoError(value.kind()), "IO ERROR")
    }
}