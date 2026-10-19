//! Each process of a run writes its output to `<runtime>/log/processes/<name>.log`. These read
//! them back: a tail for `parcela logs`, and a follower for `logs --follow` and the owner's echo.
use anyhow::{Context, Result, ensure};
use std::{
    collections::BTreeMap,
    fs::{File, OpenOptions},
    io::{Read, Seek, SeekFrom},
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
};

/// Where a run's processes write, under the environment's runtime directory.
pub fn directory(runtime: &Path) -> PathBuf {
    runtime.join("log/processes")
}

/// The file `name` writes to, opened for appending.
pub fn open(directory: &Path, name: &str) -> Result<File> {
    ensure!(valid(name), "invalid process name {name:?}");
    OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(directory.join(format!("{name}.log")))
        .with_context(|| format!("open the log of {name}"))
}

/// Empties the directory for a new run, so `logs` shows only processes of the latest one.
pub fn reset(directory: &Path) -> Result<()> {
    crate::private::directory(directory)?;
    for entry in std::fs::read_dir(directory)? {
        let entry = entry?;
        if entry.file_type()?.is_file() {
            std::fs::remove_file(entry.path())?;
        }
    }
    Ok(())
}

/// The processes with a log, sorted, so each keeps one color everywhere.
pub fn names(directory: &Path) -> Result<Vec<String>> {
    let mut names = Vec::new();
    let entries = match std::fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(names),
        Err(error) => return Err(error.into()),
    };
    for entry in entries {
        let entry = entry?;
        if let Some(name) = entry
            .file_name()
            .to_str()
            .and_then(|name| name.strip_suffix(".log"))
            && valid(name)
            && entry.file_type()?.is_file()
        {
            names.push(name.to_owned());
        }
    }
    names.sort();
    Ok(names)
}

/// The last `count` lines `name` wrote.
pub fn tail(directory: &Path, name: &str, count: usize) -> Result<Vec<String>> {
    ensure!(valid(name), "invalid process name {name:?}");
    let mut bytes = Vec::new();
    read_private(&directory.join(format!("{name}.log")))?.read_to_end(&mut bytes)?;
    let lines: Vec<String> = String::from_utf8_lossy(&bytes).lines().map(clean).collect();
    Ok(lines[lines.len().saturating_sub(count)..].to_vec())
}

/// Reads new complete lines from every log in a directory, including logs that appear later.
pub struct Follower {
    directory: PathBuf,
    files: BTreeMap<String, Followed>,
    /// Logs present when following started are read from their end.
    from_end: bool,
    /// Copy a log to `<name>.log.1` and empty it once it grows past this many bytes.
    limit: Option<u64>,
}
struct Followed {
    file: File,
    offset: u64,
    partial: Vec<u8>,
}
impl Follower {
    pub fn new(directory: &Path, from_end: bool, limit: Option<u64>) -> Self {
        Self {
            directory: directory.to_owned(),
            files: BTreeMap::new(),
            from_end,
            limit,
        }
    }

    /// Calls `emit` with each new line, in name order per call. Missing logs are skipped.
    pub fn poll(&mut self, mut emit: impl FnMut(&str, &str)) -> Result<()> {
        for name in names(&self.directory)? {
            if !self.files.contains_key(&name) {
                let Ok(mut file) = read_private(&self.directory.join(format!("{name}.log"))) else {
                    continue;
                };
                let offset = if self.from_end {
                    file.seek(SeekFrom::End(0))?
                } else {
                    0
                };
                self.files.insert(
                    name.clone(),
                    Followed {
                        file,
                        offset,
                        partial: Vec::new(),
                    },
                );
            }
        }
        self.from_end = false;
        for (name, followed) in &mut self.files {
            let length = followed.file.metadata()?.len();
            // Emptied by rotation or a new run: start again from the top.
            if length < followed.offset {
                followed.offset = 0;
                followed.partial.clear();
            }
            followed.file.seek(SeekFrom::Start(followed.offset))?;
            let mut bytes = Vec::new();
            followed.file.read_to_end(&mut bytes)?;
            followed.offset += bytes.len() as u64;
            followed.partial.extend_from_slice(&bytes);
            while let Some(end) = followed.partial.iter().position(|byte| *byte == b'\n') {
                let line: Vec<u8> = followed.partial.drain(..=end).collect();
                emit(name, &clean(&String::from_utf8_lossy(&line[..end])));
            }
            if let Some(limit) = self.limit
                && followed.offset > limit
            {
                let path = self.directory.join(format!("{name}.log"));
                std::fs::copy(&path, self.directory.join(format!("{name}.log.1")))?;
                // The writer appends, so it continues at the start of the emptied file.
                OpenOptions::new()
                    .write(true)
                    .custom_flags(libc::O_NOFOLLOW)
                    .open(&path)?
                    .set_len(0)?;
                followed.offset = 0;
            }
        }
        Ok(())
    }
}

fn read_private(path: &Path) -> Result<File> {
    Ok(OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)?)
}

/// Names are manifest keys, hooks or `parcela-` publishers: no path separators or dots.
fn valid(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
}

/// A line without a trailing carriage return or control characters other than tabs and color
/// codes, so a process cannot move the reader's cursor or set its terminal title.
fn clean(line: &str) -> String {
    let line = line.strip_suffix('\r').unwrap_or(line);
    let mut text = String::with_capacity(line.len());
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            // Keep SGR color sequences, drop every other escape sequence.
            '\x1b' if chars.peek() == Some(&'[') => {
                let mut sequence = String::from("\x1b[");
                chars.next();
                while let Some(&next) = chars.peek() {
                    sequence.push(next);
                    chars.next();
                    if next.is_ascii_alphabetic() || next == '~' {
                        break;
                    }
                }
                if sequence.ends_with('m') {
                    text.push_str(&sequence);
                }
            }
            '\x1b' if chars.peek() == Some(&']') => {
                while let Some(next) = chars.next() {
                    if next == '\x07' || (next == '\x1b' && chars.next_if_eq(&'\\').is_some()) {
                        break;
                    }
                }
            }
            '\t' => text.push(c),
            c if c.is_control() => {}
            c => text.push(c),
        }
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn lines_keep_colors_and_lose_titles_and_cursor_moves() {
        assert_eq!(clean("\x1b[32mok\x1b[0m\r"), "\x1b[32mok\x1b[0m");
        assert_eq!(clean("\x1b]0;title\x07[setup]\tbroken"), "[setup]\tbroken");
        assert_eq!(clean("a\x1b[2Kb\x1b]2;t\x1b\\c"), "abc");
    }

    #[test]
    fn a_follower_reads_new_lines_and_starts_over_after_rotation() {
        let temp = tempfile::TempDir::new().unwrap();
        let directory = temp.path();
        let mut web = open(directory, "web").unwrap();
        writeln!(web, "old").unwrap();
        let mut follower = Follower::new(directory, true, Some(16));
        let mut seen = Vec::new();
        follower
            .poll(|name, line| seen.push(format!("{name}:{line}")))
            .unwrap();
        assert!(seen.is_empty());
        write!(web, "one\ntw").unwrap();
        let mut api = open(directory, "api").unwrap();
        writeln!(api, "up").unwrap();
        follower
            .poll(|name, line| seen.push(format!("{name}:{line}")))
            .unwrap();
        assert_eq!(seen, ["api:up", "web:one"]);
        writeln!(web, "o and more than the limit").unwrap();
        follower
            .poll(|name, line| seen.push(format!("{name}:{line}")))
            .unwrap();
        assert_eq!(seen.last().unwrap(), "web:two and more than the limit");
        assert_eq!(
            std::fs::metadata(directory.join("web.log")).unwrap().len(),
            0
        );
        assert!(directory.join("web.log.1").exists());
        writeln!(web, "after").unwrap();
        follower
            .poll(|name, line| seen.push(format!("{name}:{line}")))
            .unwrap();
        assert_eq!(seen.last().unwrap(), "web:after");
        assert_eq!(names(directory).unwrap(), ["api", "web"]);
        assert_eq!(tail(directory, "web", 5).unwrap(), ["after"]);
    }
}
