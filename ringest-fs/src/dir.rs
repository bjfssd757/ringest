use std::fs::{self, DirEntry};

use dashmap::DashMap;

use crate::{error::Error, file::File};

pub struct Directory {
    pub subdirectories: DashMap<String, DirEntry>,
    pub subfiles: DashMap<String, File>,
}

impl Directory {
    pub fn new(path: &String) -> Result<Self, Error> {
        let entries = fs::read_dir(path)?;
        let subfiles: DashMap<String, File> = DashMap::new();

        for entry in entries {
            let entry = entry?;
            let pathbuf = entry.path();
            let meta = entry.metadata()?;

            if pathbuf.is_file() {
                let file = File::open(path)?;
            }
        }

        Ok(Self {
            subdirectories: DashMap::new(),
            subfiles: DashMap::new(),
        })
    }
}