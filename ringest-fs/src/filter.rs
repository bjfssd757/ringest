use std::{fs::Metadata, ops::{Bound, RangeBounds}, time::SystemTime};

#[derive(Default)]
pub struct Filter {
    pub(crate) target_name: Option<String>,
    pub(crate) name_prefix: Option<String>,
    pub(crate) name_suffix: Option<String>,
    pub(crate) target_path: Option<String>,
    pub(crate) recursive: bool,
    pub(crate) recursive_depth: Option<u64>,
    pub(crate) created_after: Option<SystemTime>,
    pub(crate) created_before: Option<SystemTime>,
    pub(crate) modified_after: Option<SystemTime>,
    pub(crate) modified_before: Option<SystemTime>,
    pub(crate) accessed_after: Option<SystemTime>,
    pub(crate) accessed_before: Option<SystemTime>,
    pub(crate) max_size: Option<u64>,
    pub(crate) min_size: Option<u64>,
    pub(crate) target_type: Option<FileType>,
    pub(crate) extension: Option<String>,
    pub(crate) access_mode: Option<AccessMode>,
    pub(crate) exclude_dirs: Option<Vec<String>>,
    pub(crate) exclude_patterns: Option<Vec<String>>,
    pub(crate) include_hidden: bool,
    pub(crate) exclude_extensions: Option<Vec<String>>,
    pub(crate) exclude_types: Option<Vec<FileType>>,
}

pub enum FileType {
    Dir,
    File,
    Symlink,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AccessMode {
    ReadOnly,
    WriteOnly,
    ReadWrite
}

pub struct FilterBuilder {
    filter: Filter
}

impl FilterBuilder {
    pub fn new() -> Self {
        Self {
            filter: Filter::default(),
        }
    }

    pub fn access(mut self, mode: AccessMode) -> Self {
        self.filter.access_mode = Some(mode);
        self
    }

    pub fn include_hidden(mut self, is_include_hidden: bool) -> Self {
        self.filter.include_hidden = is_include_hidden;
        self
    }

    pub fn exclude_types<I, S>(mut self, types: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<FileType>,
    {
        let types = types.into_iter().map(|t| t.into());
        
        match self.filter.exclude_types.as_mut() {
            Some(ex) => ex.extend(types),
            None => self.filter.exclude_types = Some(types.collect()),
        }
        self
    }

    pub fn exclude_dirs<I, S>(mut self, dirs: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let dirs = dirs.into_iter().map(|d| d.into());
        
        match self.filter.exclude_dirs.as_mut() {
            Some(ex) => ex.extend(dirs),
            None => self.filter.exclude_dirs = Some(dirs.collect()),
        }
        self
    }

    pub fn exclude_extensions<I, S>(mut self, extensions: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let extensions = extensions.into_iter().map(|e| e.into());
        
        match self.filter.exclude_extensions.as_mut() {
            Some(ex) => ex.extend(extensions),
            None => self.filter.exclude_extensions = Some(extensions.collect()),
        }
        self
    }

    pub fn exclude_patterns<I, S>(mut self, patterns: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let patterns = patterns.into_iter().map(|p| p.into());
        
        match self.filter.exclude_patterns.as_mut() {
            Some(ex) => ex.extend(patterns),
            None => self.filter.exclude_patterns = Some(patterns.collect()),
        }
        self
    }

    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.filter.target_name = Some(name.into());
        self
    }

    pub fn target_path(mut self, path: impl Into<String>) -> Self {
        self.filter.target_path = Some(path.into());
        self
    }

    pub fn target_extension(mut self, extension: impl Into<String>) -> Self {
        self.filter.extension = Some(extension.into());
        self
    }

    pub fn target_type(mut self, ty: FileType) -> Self {
        self.filter.target_type = Some(ty);
        self
    }

    pub fn name_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.filter.name_prefix = Some(prefix.into());
        self
    }

    pub fn name_suffix(mut self, suffix: impl Into<String>) -> Self {
        self.filter.name_suffix = Some(suffix.into());
        self
    }

    pub fn recursive(mut self, depth: u64) -> Self {
        self.filter.recursive = true;
        self.filter.recursive_depth = Some(depth);
        self
    }

    pub fn size_limit<R>(mut self, range: R) -> Self
    where
        R: RangeBounds<u64>
    {
        self.filter.min_size = match range.start_bound() {
            Bound::Included(&s) => Some(s),
            Bound::Excluded(&s) => Some(s + 1),
            Bound::Unbounded => None,
        };

        self.filter.max_size = match range.end_bound() {
            Bound::Included(&e) => Some(e),
            Bound::Excluded(&e) => Some(e.saturating_sub(1)),
            Bound::Unbounded => None,
        };

        self
    }

    pub fn build(self) -> Filter {
        self.filter
    }
}

impl Filter {
    pub fn builder() -> FilterBuilder {
        FilterBuilder::new()
    }

    #[inline]
    pub fn check_modified(&self, file_time: SystemTime) -> bool {
        if let Some(after) = self.modified_after {
            if file_time < after {
                return false
            }
        }
        if let Some(before) = self.modified_before {
            if file_time > before {
                return false
            }
        }
        true
    }

    #[inline]
    pub fn check_accessed(&self, file_time: SystemTime) -> bool {
        if let Some(after) = self.accessed_after {
            if file_time < after {
                return false
            }
        }
        if let Some(before) = self.accessed_before {
            if file_time > before {
                return false
            }
        }
        true
    }

    #[inline]
    pub fn check_created(&self, file_time: SystemTime) -> bool {
        if let Some(after) = self.created_after {
            if file_time < after {
                return false
            }
        }
        if let Some(before) = self.created_before {
            if file_time > before {
                return false
            }
        }
        true
    }

    #[inline]
    pub fn check_all_ranges(&self, file_time: SystemTime) -> bool {
        self.check_accessed(file_time) && self.check_created(file_time) && self.check_modified(file_time)
    }

    pub fn matches_access(&self, metadata: &Metadata) -> bool {
        let Some(target_mode) = self.access_mode else { return true };

        let permissions = metadata.permissions();
        let is_readonly = permissions.readonly();

        match target_mode {
            AccessMode::ReadOnly => is_readonly,
            AccessMode::WriteOnly | AccessMode::ReadWrite => {
                if cfg!(windows) {
                    !is_readonly
                } else {
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::PermissionsExt;
                        let mode = permissions.mode();
                        (mode & 0o222) != 0
                    }
                    #[cfg(not(unix))]
                    !is_readonly
                }
            }
        }
    }
}