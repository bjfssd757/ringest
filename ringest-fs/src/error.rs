
#[derive(Debug)]
pub enum ErrorKind {
    IoError(std::io::ErrorKind),
    SearchError(SearchErrorKind)
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
    pub fn new(kind: ErrorKind) -> Self {
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
        Error::new(ErrorKind::IoError(value.kind()))
    }
}