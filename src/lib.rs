//! Recursively copy a directory from a to b.

use log::*;
use std::fs::{copy, read_link};
use std::io::{Error, ErrorKind};
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use walkdir::WalkDir;

#[derive(Debug, Clone)]
/// Recursively copy a directory from a to b.
pub struct CopyBuilder {
    /// The source directory
    pub source: PathBuf,
    /// The destination directory
    pub destination: PathBuf,
    /// Overwrite all files in target, if already existing
    overwrite_all: bool,
    /// Overwrite target files if they are newer
    overwrite_if_newer: bool,
    /// Overwrite target files if they differ in size
    overwrite_if_size_differs: bool,
    /// A list of include filters
    exclude_filters: Vec<String>,
    /// A list of exclude filters
    include_filters: Vec<String>,
}

/// Determine if the modification date of file_a is newer than that of file_b
fn is_file_newer(file_a: &Path, file_b: &Path) -> bool {
    match (file_a.symlink_metadata(), file_b.symlink_metadata()) {
        (Ok(meta_a), Ok(meta_b)) => {
            meta_a.modified().unwrap_or_else(|_| SystemTime::now())
                > meta_b.modified().unwrap_or(SystemTime::UNIX_EPOCH)
        }
        _ => false,
    }
}

/// Determine if file_a and file_b's size differs.
fn is_filesize_different(file_a: &Path, file_b: &Path) -> bool {
    match (file_a.symlink_metadata(), file_b.symlink_metadata()) {
        (Ok(meta_a), Ok(meta_b)) => meta_a.len() != meta_b.len(),
        _ => false,
    }
}

impl CopyBuilder {
    /// Construct a new CopyBuilder with `source` and `dest`.
    pub fn new<P: AsRef<Path>, Q: AsRef<Path>>(source: P, dest: Q) -> CopyBuilder {
        CopyBuilder {
            source: source.as_ref().to_path_buf(),
            destination: dest.as_ref().to_path_buf(),
            overwrite_all: false,
            overwrite_if_newer: false,
            overwrite_if_size_differs: false,
            exclude_filters: vec![],
            include_filters: vec![],
        }
    }

    /// Overwrite target files (off by default)
    pub fn overwrite(self, overwrite: bool) -> CopyBuilder {
        CopyBuilder {
            overwrite_all: overwrite,
            ..self
        }
    }

    /// Overwrite if the source is newer (off by default)
    pub fn overwrite_if_newer(self, overwrite_only_newer: bool) -> CopyBuilder {
        CopyBuilder {
            overwrite_if_newer: overwrite_only_newer,
            ..self
        }
    }

    /// Overwrite if size between source and dest differs (off by default)
    pub fn overwrite_if_size_differs(self, overwrite_if_size_differs: bool) -> CopyBuilder {
        CopyBuilder {
            overwrite_if_size_differs,
            ..self
        }
    }

    /// Do not copy files that contain this string
    pub fn with_exclude_filter(self, f: &str) -> CopyBuilder {
        let mut filters = self.exclude_filters.clone();
        filters.push(f.to_owned());
        CopyBuilder {
            exclude_filters: filters,
            ..self
        }
    }

    /// Only copy files that contain this string.
    pub fn with_include_filter(self, f: &str) -> CopyBuilder {
        let mut filters = self.include_filters.clone();
        filters.push(f.to_owned());
        CopyBuilder {
            include_filters: filters,
            ..self
        }
    }

    /// Execute the copy operation
    pub fn run(&self) -> Result<(), std::io::Error> {
        if !self.destination.is_dir() {
            debug!("MKDIR {:?}", &self.destination);
            std::fs::create_dir_all(&self.destination)?;
        }
        let abs_source = self.source.canonicalize()?;
        let abs_dest = self.destination.canonicalize()?;
        debug!(
            "Building copy operation: SRC {} DST {}",
            abs_source.display(),
            abs_dest.display()
        );

        'files: for entry in WalkDir::new(&abs_source)
            .into_iter()
            .filter_entry(|e| e.path() != abs_dest)
            .filter_map(|e| e.ok())
        {
            let rel_dest = entry.path().strip_prefix(&abs_source).map_err(|e| {
                Error::new(ErrorKind::Other, format!("Could not strip prefix: {:?}", e))
            })?;
            let dest_entry = abs_dest.join(rel_dest);

            if entry.path().symlink_metadata().is_ok() && !entry.file_type().is_dir() {
                // the source exists, but isn't a directory

                // Early out if target is present and overwrite is off
                if !self.overwrite_all
                    && dest_entry.symlink_metadata().is_ok()
                    && !self.overwrite_if_newer
                    && !self.overwrite_if_size_differs
                {
                    continue;
                }

                for f in &self.exclude_filters {
                    debug!("EXCL {} for {:?}", f, entry);

                    if entry.path().to_string_lossy().contains(f) {
                        continue 'files;
                    }
                }

                if !self.include_filters.is_empty()
                    && !self
                        .include_filters
                        .iter()
                        .any(|f| entry.path().to_string_lossy().contains(f))
                {
                    continue 'files;
                }

                // File is not present: copy it in any case
                let dest_exists = dest_entry.symlink_metadata().is_ok();

                if !dest_exists {
                    debug!(
                        "Dest not present: CP {} DST {}",
                        entry.path().display(),
                        dest_entry.display()
                    );
                }

                // File newer?
                if dest_exists && self.overwrite_if_newer {
                    if is_file_newer(entry.path(), &dest_entry) {
                        debug!(
                            "Source newer: CP {} DST {}",
                            entry.path().display(),
                            dest_entry.display()
                        );
                    } else {
                        continue;
                    }
                }

                // Different size?
                if dest_exists && self.overwrite_if_size_differs {
                    if is_filesize_different(entry.path(), &dest_entry) {
                        debug!(
                            "Source differs: CP {} DST {}",
                            entry.path().display(),
                            dest_entry.display()
                        );
                    } else {
                        continue;
                    }
                }

                if entry.file_type().is_file() {
                    // The regular copy operation
                    debug!("CP {} DST {}", entry.path().display(), dest_entry.display());
                    copy(entry.path(), dest_entry)?;
                } else if entry.file_type().is_symlink() {
                    debug!(
                        "CP LNK {} DST {}",
                        entry.path().display(),
                        dest_entry.display()
                    );
                    let target = read_link(entry.path())?;
                    #[cfg(unix)]
                    std::os::unix::fs::symlink(target, dest_entry)?
                } else {
                    eprintln!(
                        "File {} has unhalded type {:?}, skipping",
                        entry.path().display(),
                        entry.file_type()
                    );
                }
            } else if entry.path().is_dir() && !dest_entry.is_dir() {
                debug!("MKDIR {}", entry.path().display());
                std::fs::create_dir_all(dest_entry)?;
            }
        }

        Ok(())
    }
}
