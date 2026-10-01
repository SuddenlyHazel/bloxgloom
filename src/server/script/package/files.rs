//! Descriptor-relative opens with NOFOLLOW on every component. In particular,
//! no check-then-open race can swap a checked path for a symlink. Special files
//! are opened nonblocking and rejected before any read.

#[cfg(unix)]
mod platform {
    use std::fs::File;
    use std::io::{self, Read};
    use std::os::fd::OwnedFd;
    use std::path::{Component, Path};

    use rustix::fs::{Dir, Mode, OFlags, open, openat};

    pub struct Directory(OwnedFd);
    const DIRECTORY: OFlags = OFlags::RDONLY
        .union(OFlags::DIRECTORY)
        .union(OFlags::NOFOLLOW)
        .union(OFlags::CLOEXEC);

    impl Directory {
        pub fn root(path: &Path) -> io::Result<Self> {
            if path.as_os_str().len() > 4096 || path.components().count() > 64 {
                return Err(io::Error::other("root path limit exceeded"));
            }
            let mut directory = Self(open(
                if path.is_absolute() { "/" } else { "." },
                DIRECTORY,
                Mode::empty(),
            )?);
            for component in path.components() {
                match component {
                    Component::RootDir | Component::CurDir => {}
                    Component::Normal(name) => {
                        directory = Self(openat(&directory.0, name, DIRECTORY, Mode::empty())?)
                    }
                    _ => return Err(io::Error::other("root cannot contain parent traversal")),
                }
            }
            Ok(directory)
        }

        pub fn child(&self, name: &str) -> io::Result<Self> {
            Ok(Self(openat(&self.0, name, DIRECTORY, Mode::empty())?))
        }

        pub fn names(&self, limit: usize) -> io::Result<Vec<String>> {
            let mut names = Vec::new();
            for entry in Dir::read_from(&self.0)? {
                let entry = entry?;
                let bytes = entry.file_name().to_bytes();
                if bytes == b"." || bytes == b".." {
                    continue;
                }
                if names.len() == limit {
                    return Err(io::Error::other("too many package directories"));
                }
                if bytes.len() > 64 {
                    return Err(io::Error::other("package directory name too long"));
                }
                names.push(
                    std::str::from_utf8(bytes)
                        .map_err(io::Error::other)?
                        .to_owned(),
                );
            }
            names.sort();
            Ok(names)
        }

        pub fn read(&self, path: &str, limit: usize) -> io::Result<String> {
            String::from_utf8(self.read_bytes(path, limit)?).map_err(io::Error::other)
        }

        pub fn read_bytes(&self, path: &str, limit: usize) -> io::Result<Vec<u8>> {
            let mut parts = path.split('/').peekable();
            let mut parent = None;
            while let Some(part) = parts.next() {
                let directory = parent.as_ref().unwrap_or(self);
                if parts.peek().is_some() {
                    parent = Some(directory.child(part)?);
                } else {
                    let fd = openat(
                        &directory.0,
                        part,
                        OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
                        Mode::empty(),
                    )?;
                    let file = File::from(fd);
                    let metadata = file.metadata()?;
                    if !metadata.is_file() {
                        return Err(io::Error::other("source must be a bounded regular file"));
                    }
                    if metadata.len() > limit as u64 {
                        return Err(io::Error::other(format!(
                            "file bytes: attempted {}; maximum {limit} (bounded regular file)",
                            metadata.len()
                        )));
                    }
                    let mut bytes = Vec::new();
                    file.take(limit as u64 + 1).read_to_end(&mut bytes)?;
                    if bytes.len() > limit {
                        return Err(io::Error::other(format!(
                            "file bytes: attempted {}; maximum {limit}",
                            bytes.len()
                        )));
                    }
                    return Ok(bytes);
                }
            }
            Err(io::Error::other("empty file path"))
        }
    }
}

#[cfg(not(unix))]
mod platform {
    use std::{io, path::Path};
    pub struct Directory;
    impl Directory {
        pub fn root(_: &Path) -> io::Result<Self> {
            Err(io::Error::other(
                "secure local package discovery is currently Unix-only",
            ))
        }
        pub fn child(&self, _: &str) -> io::Result<Self> {
            unreachable!()
        }
        pub fn names(&self, _: usize) -> io::Result<Vec<String>> {
            unreachable!()
        }
        pub fn read(&self, _: &str, _: usize) -> io::Result<String> {
            unreachable!()
        }
        pub fn read_bytes(&self, _: &str, _: usize) -> io::Result<Vec<u8>> {
            unreachable!()
        }
    }
}

pub(super) use platform::Directory;
