//! Online SQLite backup over an acknowledged SSH file channel, without local staging.

use crate::{private, transfer};
use anyhow::{Context, Result, bail, ensure};
use rusqlite::{
    Connection, OpenFlags,
    backup::{Backup, StepResult},
    ffi,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    ffi::{CStr, CString, c_char, c_int, c_void},
    fs::{self, File},
    io::{Read, Write},
    os::{
        fd::{AsRawFd, RawFd},
        unix::{ffi::OsStrExt, fs::FileExt},
    },
    path::{Path, PathBuf},
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
    ptr,
    time::{Duration, Instant},
};

const WRITE: u8 = 1;
const READ: u8 = 2;
const TRUNCATE: u8 = 3;
const SYNC: u8 = 4;
const FINISH: u8 = 5;
const BLOCK: usize = 4 * 1024 * 1024;

#[derive(Debug, Serialize)]
pub struct Streamed {
    pub sha256: String,
    pub bytes: u64,
    pub source_snapshot: &'static str,
    pub transport_integrity: &'static str,
    pub activation: &'static str,
}

fn number(input: &mut impl Read) -> Result<u64> {
    let mut buffer = [0; 8];
    input.read_exact(&mut buffer)?;
    Ok(u64::from_le_bytes(buffer))
}

fn request(output: &mut impl Write, operation: u8, offset: u64, bytes: u64) -> Result<()> {
    output.write_all(&[operation])?;
    output.write_all(&offset.to_le_bytes())?;
    output.write_all(&bytes.to_le_bytes())?;
    Ok(())
}

fn hash(path: &Path) -> Result<Streamed> {
    let mut input = File::open(path)?;
    let bytes = input.metadata()?.len();
    let mut digest = Sha256::new();
    let mut buffer = [0; 65536];
    loop {
        let read = input.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    Ok(Streamed {
        sha256: format!("{:x}", digest.finalize()),
        bytes,
        source_snapshot: "SQLite online backup of a pinned read transaction",
        transport_integrity: "per-write SHA-256 acknowledgment; receiver full-file SHA-256",
        activation: "paused; no ownership transfer",
    })
}

pub fn receive(output: &Path, max_bytes: u64) -> Result<()> {
    let parent = output
        .parent()
        .context("stream output needs a parent directory")?;
    ensure!(
        fs::symlink_metadata(parent)?.is_dir(),
        "receiver parent must exist"
    );
    private::directory(parent)?;
    ensure!(
        !output.try_exists()? && fs::symlink_metadata(output).is_err(),
        "receiver never overwrites existing files"
    );
    ensure!(
        max_bytes > 0 && max_bytes <= available(parent)?,
        "insufficient receiver space for declared stream size"
    );
    let stage = tempfile::Builder::new()
        .prefix(".parcela-sqlite-stream-")
        .tempfile_in(parent)?;
    let mut input = std::io::stdin().lock();
    let mut response = std::io::stdout().lock();
    response.write_all(&[0])?;
    response.flush()?;
    loop {
        let mut operation = [0];
        input
            .read_exact(&mut operation)
            .context("interrupted SQLite stream; incomplete private artifact discarded")?;
        let offset = number(&mut input)?;
        let bytes = number(&mut input)?;
        match operation[0] {
            WRITE => {
                ensure!(
                    bytes > 0
                        && bytes <= BLOCK as u64
                        && offset
                            .checked_add(bytes)
                            .is_some_and(|end| end <= max_bytes),
                    "stream write outside declared bounds"
                );
                let mut expected = [0; 32];
                input.read_exact(&mut expected)?;
                let mut buffer = vec![0; bytes as usize];
                input.read_exact(&mut buffer)?;
                let digest: [u8; 32] = Sha256::digest(&buffer).into();
                ensure!(digest == expected, "stream write integrity failure");
                stage.as_file().write_all_at(&buffer, offset)?;
                response.write_all(&digest)?;
            }
            READ => {
                ensure!(
                    bytes > 0
                        && bytes <= BLOCK as u64
                        && offset
                            .checked_add(bytes)
                            .is_some_and(|end| end <= max_bytes),
                    "stream read outside declared bounds"
                );
                let mut buffer = vec![0; bytes as usize];
                let read = stage.as_file().read_at(&mut buffer, offset)?;
                response.write_all(&(read as u64).to_le_bytes())?;
                response.write_all(&buffer[..read])?;
            }
            TRUNCATE => {
                ensure!(
                    offset <= max_bytes && bytes == 0,
                    "stream truncate outside declared bounds"
                );
                stage.as_file().set_len(offset)?;
                response.write_all(&[0])?;
            }
            SYNC => {
                ensure!(offset == 0 && bytes == 0, "invalid sync request");
                stage.as_file().sync_all()?;
                response.write_all(&[0])?;
            }
            FINISH => {
                ensure!(offset == 0 && bytes == 0, "invalid finish request");
                stage.as_file().sync_all()?;
                let database =
                    Connection::open_with_flags(stage.path(), OpenFlags::SQLITE_OPEN_READ_WRITE)?;
                database.pragma_update(None, "journal_mode", "DELETE")?;
                let check: String =
                    database.query_row("PRAGMA quick_check(1)", [], |row| row.get(0))?;
                ensure!(
                    check == "ok",
                    "received SQLite snapshot failed validation: {check}"
                );
                database.close().map_err(|(_, error)| error)?;
                let result = hash(stage.path())?;
                let digest = (0..64)
                    .step_by(2)
                    .map(|index| u8::from_str_radix(&result.sha256[index..index + 2], 16))
                    .collect::<std::result::Result<Vec<_>, _>>()?;
                stage
                    .persist_noclobber(output)
                    .map_err(|error| error.error)?;
                File::open(parent)?.sync_all()?;
                response.write_all(&result.bytes.to_le_bytes())?;
                response.write_all(&digest)?;
                response.flush()?;
                return Ok(());
            }
            _ => bail!("unsupported stream operation"),
        }
        response.flush()?;
    }
}

#[allow(clippy::unnecessary_cast)] // statvfs field widths differ between macOS and Linux.
fn available(path: &Path) -> Result<u64> {
    let path = CString::new(path.as_os_str().as_bytes())?;
    let mut stat = std::mem::MaybeUninit::<libc::statvfs>::uninit();
    // statvfs writes the initialized value only on success; CString owns the C path.
    ensure!(
        unsafe { libc::statvfs(path.as_ptr(), stat.as_mut_ptr()) } == 0,
        "cannot measure available disk space"
    );
    let stat = unsafe { stat.assume_init() };
    Ok((stat.f_bavail as u64).saturating_mul(stat.f_frsize as u64))
}

fn nonblocking(fd: RawFd) -> Result<()> {
    // The retained child owns the descriptor. fcntl does not retain any pointers.
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    ensure!(
        flags >= 0 && unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } == 0,
        "cannot bound stream pipe I/O"
    );
    Ok(())
}

fn pending(fd: RawFd, events: libc::c_short, deadline: Instant, source: &Path) -> Result<()> {
    loop {
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .context("SQLite stream deadline exceeded")?;
        ensure!(
            available(source)? > 512 * 1024 * 1024,
            "source disk critically low; abort snapshot to release retained WAL pages"
        );
        let mut descriptor = libc::pollfd {
            fd,
            events,
            revents: 0,
        };
        // poll borrows this one descriptor only during the call.
        let result = unsafe {
            libc::poll(
                &mut descriptor,
                1,
                remaining.as_millis().min(30_000) as c_int,
            )
        };
        if result > 0 {
            return Ok(());
        }
        if result < 0 && std::io::Error::last_os_error().kind() != std::io::ErrorKind::Interrupted {
            return Err(std::io::Error::last_os_error().into());
        }
    }
}

fn read_pipe(
    input: &mut ChildStdout,
    mut buffer: &mut [u8],
    deadline: Instant,
    source: &Path,
) -> Result<()> {
    while !buffer.is_empty() {
        pending(input.as_raw_fd(), libc::POLLIN, deadline, source)?;
        match input.read(buffer) {
            Ok(0) => bail!("receiver closed before acknowledgment"),
            Ok(count) => {
                buffer = &mut buffer[count..];
            }
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                ) => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

fn write_pipe(
    output: &mut ChildStdin,
    mut buffer: &[u8],
    deadline: Instant,
    source: &Path,
) -> Result<()> {
    while !buffer.is_empty() {
        pending(output.as_raw_fd(), libc::POLLOUT, deadline, source)?;
        match output.write(buffer) {
            Ok(0) => bail!("receiver stopped accepting stream writes"),
            Ok(count) => {
                buffer = &buffer[count..];
            }
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                ) => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

struct Channel {
    child: Child,
    input: ChildStdin,
    output: ChildStdout,
    size: u64,
    buffer: Vec<u8>,
    offset: u64,
    error: Option<String>,
    deadline: Instant,
    source_parent: PathBuf,
}

impl Channel {
    fn open(command: &mut Command, source_parent: &Path) -> Result<Self> {
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()?;
        let input = child.stdin.take().context("missing receiver input")?;
        let output = child.stdout.take().context("missing receiver output")?;
        let mut channel = Self {
            child,
            input,
            output,
            size: 0,
            buffer: Vec::with_capacity(BLOCK),
            offset: 0,
            error: None,
            deadline: Instant::now() + Duration::from_secs(3600),
            source_parent: source_parent.into(),
        };
        nonblocking(channel.input.as_raw_fd())?;
        nonblocking(channel.output.as_raw_fd())?;
        channel.acknowledge()?;
        Ok(channel)
    }
    fn send_request(&mut self, operation: u8, offset: u64, bytes: u64) -> Result<()> {
        let mut frame = Vec::with_capacity(17);
        request(&mut frame, operation, offset, bytes)?;
        write_pipe(&mut self.input, &frame, self.deadline, &self.source_parent)
    }
    fn receive_number(&mut self) -> Result<u64> {
        let mut buffer = [0; 8];
        read_pipe(
            &mut self.output,
            &mut buffer,
            self.deadline,
            &self.source_parent,
        )?;
        Ok(u64::from_le_bytes(buffer))
    }
    fn acknowledge(&mut self) -> Result<()> {
        let mut status = [0];
        read_pipe(
            &mut self.output,
            &mut status,
            self.deadline,
            &self.source_parent,
        )?;
        ensure!(status == [0], "receiver rejected the stream");
        Ok(())
    }
    fn flush(&mut self) -> Result<()> {
        if self.buffer.is_empty() {
            return Ok(());
        }
        let digest: [u8; 32] = Sha256::digest(&self.buffer).into();
        self.send_request(WRITE, self.offset, self.buffer.len() as u64)?;
        write_pipe(&mut self.input, &digest, self.deadline, &self.source_parent)?;
        write_pipe(
            &mut self.input,
            &self.buffer,
            self.deadline,
            &self.source_parent,
        )?;
        self.input.flush()?;
        let mut received = [0; 32];
        read_pipe(
            &mut self.output,
            &mut received,
            self.deadline,
            &self.source_parent,
        )?;
        ensure!(
            digest == received,
            "receiver did not acknowledge the exact write"
        );
        self.buffer.clear();
        Ok(())
    }
    fn write(&mut self, bytes: &[u8], offset: u64) -> Result<()> {
        ensure!(bytes.len() <= BLOCK, "SQLite write exceeds stream bound");
        if self.offset + self.buffer.len() as u64 != offset
            || self.buffer.len() + bytes.len() > BLOCK
        {
            self.flush()?;
        }
        if self.buffer.is_empty() {
            self.offset = offset;
        }
        self.buffer.extend_from_slice(bytes);
        self.size = self.size.max(
            offset
                .checked_add(bytes.len() as u64)
                .context("stream offset overflow")?,
        );
        Ok(())
    }
    fn read(&mut self, bytes: &mut [u8], offset: u64) -> Result<bool> {
        self.flush()?;
        self.send_request(READ, offset, bytes.len() as u64)?;
        self.input.flush()?;
        let received = self.receive_number()?;
        ensure!(
            received <= bytes.len() as u64,
            "invalid read acknowledgment"
        );
        read_pipe(
            &mut self.output,
            &mut bytes[..received as usize],
            self.deadline,
            &self.source_parent,
        )?;
        bytes[received as usize..].fill(0);
        Ok(received == bytes.len() as u64)
    }
    fn truncate(&mut self, size: u64) -> Result<()> {
        self.flush()?;
        self.send_request(TRUNCATE, size, 0)?;
        self.input.flush()?;
        self.acknowledge()?;
        self.size = size;
        Ok(())
    }
    fn sync(&mut self) -> Result<()> {
        self.flush()?;
        self.send_request(SYNC, 0, 0)?;
        self.input.flush()?;
        self.acknowledge()
    }
    fn finish(&mut self) -> Result<Streamed> {
        self.sync()?;
        self.send_request(FINISH, 0, 0)?;
        self.input.flush()?;
        let bytes = self.receive_number()?;
        let mut digest = [0; 32];
        read_pipe(
            &mut self.output,
            &mut digest,
            self.deadline,
            &self.source_parent,
        )?;
        ensure!(
            self.child.wait()?.success(),
            "receiver failed after snapshot commit"
        );
        ensure!(bytes == self.size, "receiver file-size mismatch");
        Ok(Streamed {
            sha256: digest.iter().map(|byte| format!("{byte:02x}")).collect(),
            bytes,
            source_snapshot: "SQLite online backup of a pinned read transaction",
            transport_integrity: "per-write SHA-256 acknowledgment; receiver full-file SHA-256",
            activation: "paused; no ownership transfer",
        })
    }
}

impl Drop for Channel {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[repr(C)]
struct RemoteFile {
    base: ffi::sqlite3_file,
    channel: *mut Channel,
}

struct Vfs {
    abi: Box<ffi::sqlite3_vfs>,
    name: CString,
}
impl Vfs {
    fn register(channel: &mut Channel) -> Result<Self> {
        let name = CString::new(format!("parcela-{}", uuid::Uuid::new_v4()))?;
        // SQLite borrows this pinned ABI and channel only until the destination closes.
        let mut abi: Box<ffi::sqlite3_vfs> = Box::new(unsafe { std::mem::zeroed() });
        abi.iVersion = 1;
        abi.szOsFile = std::mem::size_of::<RemoteFile>() as c_int;
        abi.mxPathname = 256;
        abi.zName = name.as_ptr();
        abi.pAppData = (channel as *mut Channel).cast();
        abi.xOpen = Some(open);
        abi.xDelete = Some(delete);
        abi.xAccess = Some(access);
        abi.xFullPathname = Some(full_path);
        abi.xRandomness = Some(random);
        abi.xSleep = Some(sleep);
        abi.xCurrentTime = Some(time);
        ensure!(
            unsafe { ffi::sqlite3_vfs_register(&mut *abi, 0) } == ffi::SQLITE_OK,
            "cannot register remote backup VFS"
        );
        Ok(Self { abi, name })
    }
}
impl Drop for Vfs {
    fn drop(&mut self) {
        unsafe { ffi::sqlite3_vfs_unregister(&mut *self.abi) };
    }
}

// Callbacks never unwind through C. RemoteFile and its borrowed channel live until SQLite closes it.
fn callback(
    file: *mut ffi::sqlite3_file,
    action: impl FnOnce(&mut Channel) -> Result<()>,
) -> c_int {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let channel = unsafe { &mut *(*file.cast::<RemoteFile>()).channel };
        match action(channel) {
            Ok(()) => ffi::SQLITE_OK,
            Err(error) => {
                channel.error = Some(error.to_string());
                ffi::SQLITE_IOERR
            }
        }
    }));
    result.unwrap_or(ffi::SQLITE_IOERR)
}
unsafe extern "C" fn close(file: *mut ffi::sqlite3_file) -> c_int {
    callback(file, Channel::flush)
}
unsafe extern "C" fn read(
    file: *mut ffi::sqlite3_file,
    buffer: *mut c_void,
    length: c_int,
    offset: i64,
) -> c_int {
    if length < 0 || offset < 0 {
        return ffi::SQLITE_IOERR;
    }
    let mut complete = false;
    let status = callback(file, |channel| {
        complete = channel.read(
            unsafe { std::slice::from_raw_parts_mut(buffer.cast(), length as usize) },
            offset as u64,
        )?;
        Ok(())
    });
    if status == ffi::SQLITE_OK && !complete {
        ffi::SQLITE_IOERR_SHORT_READ
    } else {
        status
    }
}
unsafe extern "C" fn write(
    file: *mut ffi::sqlite3_file,
    buffer: *const c_void,
    length: c_int,
    offset: i64,
) -> c_int {
    if length < 0 || offset < 0 {
        return ffi::SQLITE_IOERR;
    }
    callback(file, |channel| {
        channel.write(
            unsafe { std::slice::from_raw_parts(buffer.cast(), length as usize) },
            offset as u64,
        )
    })
}
unsafe extern "C" fn truncate(file: *mut ffi::sqlite3_file, size: i64) -> c_int {
    if size < 0 {
        return ffi::SQLITE_IOERR;
    }
    callback(file, |channel| channel.truncate(size as u64))
}
unsafe extern "C" fn sync(file: *mut ffi::sqlite3_file, _: c_int) -> c_int {
    callback(file, Channel::sync)
}
unsafe extern "C" fn size(file: *mut ffi::sqlite3_file, size: *mut i64) -> c_int {
    callback(file, |channel| {
        unsafe { *size = i64::try_from(channel.size)? };
        Ok(())
    })
}
unsafe extern "C" fn lock(_: *mut ffi::sqlite3_file, _: c_int) -> c_int {
    ffi::SQLITE_OK
}
unsafe extern "C" fn reserved(_: *mut ffi::sqlite3_file, result: *mut c_int) -> c_int {
    unsafe { *result = 0 };
    ffi::SQLITE_OK
}
unsafe extern "C" fn control(_: *mut ffi::sqlite3_file, _: c_int, _: *mut c_void) -> c_int {
    ffi::SQLITE_NOTFOUND
}
unsafe extern "C" fn sector(_: *mut ffi::sqlite3_file) -> c_int {
    4096
}
unsafe extern "C" fn characteristics(_: *mut ffi::sqlite3_file) -> c_int {
    0
}
static METHODS: ffi::sqlite3_io_methods = ffi::sqlite3_io_methods {
    iVersion: 1,
    xClose: Some(close),
    xRead: Some(read),
    xWrite: Some(write),
    xTruncate: Some(truncate),
    xSync: Some(sync),
    xFileSize: Some(size),
    xLock: Some(lock),
    xUnlock: Some(lock),
    xCheckReservedLock: Some(reserved),
    xFileControl: Some(control),
    xSectorSize: Some(sector),
    xDeviceCharacteristics: Some(characteristics),
    xShmMap: None,
    xShmLock: None,
    xShmBarrier: None,
    xShmUnmap: None,
    xFetch: None,
    xUnfetch: None,
};
unsafe extern "C" fn open(
    vfs: *mut ffi::sqlite3_vfs,
    _: *const c_char,
    file: *mut ffi::sqlite3_file,
    flags: c_int,
    output: *mut c_int,
) -> c_int {
    if flags & ffi::SQLITE_OPEN_MAIN_DB == 0 {
        return ffi::SQLITE_CANTOPEN;
    }
    unsafe {
        ptr::write(
            file.cast::<RemoteFile>(),
            RemoteFile {
                base: ffi::sqlite3_file { pMethods: &METHODS },
                channel: (*vfs).pAppData.cast(),
            },
        );
        if !output.is_null() {
            *output = flags;
        }
    }
    ffi::SQLITE_OK
}
unsafe extern "C" fn delete(_: *mut ffi::sqlite3_vfs, _: *const c_char, _: c_int) -> c_int {
    ffi::SQLITE_IOERR_DELETE
}
unsafe extern "C" fn access(
    _: *mut ffi::sqlite3_vfs,
    _: *const c_char,
    _: c_int,
    result: *mut c_int,
) -> c_int {
    unsafe { *result = 0 };
    ffi::SQLITE_OK
}
unsafe extern "C" fn full_path(
    _: *mut ffi::sqlite3_vfs,
    name: *const c_char,
    capacity: c_int,
    output: *mut c_char,
) -> c_int {
    let name = unsafe { CStr::from_ptr(name) }.to_bytes_with_nul();
    if name.len() > capacity as usize {
        return ffi::SQLITE_CANTOPEN;
    }
    unsafe { ptr::copy_nonoverlapping(name.as_ptr().cast(), output, name.len()) };
    ffi::SQLITE_OK
}
unsafe extern "C" fn random(_: *mut ffi::sqlite3_vfs, count: c_int, output: *mut c_char) -> c_int {
    let default = unsafe { ffi::sqlite3_vfs_find(ptr::null()) };
    unsafe { ((*default).xRandomness.unwrap())(default, count, output) }
}
unsafe extern "C" fn sleep(_: *mut ffi::sqlite3_vfs, micros: c_int) -> c_int {
    let default = unsafe { ffi::sqlite3_vfs_find(ptr::null()) };
    unsafe { ((*default).xSleep.unwrap())(default, micros) }
}
unsafe extern "C" fn time(_: *mut ffi::sqlite3_vfs, value: *mut f64) -> c_int {
    let default = unsafe { ffi::sqlite3_vfs_find(ptr::null()) };
    unsafe { ((*default).xCurrentTime.unwrap())(default, value) }
}

pub fn backup_over_command(source: &Path, command: &mut Command) -> Result<Streamed> {
    ensure!(
        fs::symlink_metadata(source)?.is_file(),
        "stream source must be a regular file"
    );
    let input = Connection::open_with_flags(source, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    input.execute_batch("BEGIN")?;
    let _: i64 = input.query_row("PRAGMA page_count", [], |row| row.get(0))?;
    backup_input(source, input, command)
}

fn backup_input(source: &Path, input: Connection, command: &mut Command) -> Result<Streamed> {
    let mut channel = Channel::open(command, source.parent().context("source parent missing")?)?;
    let vfs = Vfs::register(&mut channel)?;
    let mut output = Connection::open_with_flags_and_vfs(
        "parcela-stream",
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE,
        vfs.name.to_str()?,
    )?;
    output.execute_batch("PRAGMA journal_mode=OFF; PRAGMA synchronous=OFF")?;
    let deadline = Instant::now() + Duration::from_secs(3600);
    {
        let backup = Backup::new(&input, &mut output)?;
        loop {
            ensure!(
                Instant::now() < deadline,
                "streamed SQLite backup exceeded one hour"
            );
            ensure!(
                available(source.parent().context("source parent missing")?)? > 512 * 1024 * 1024,
                "source disk critically low; abort snapshot to release retained WAL pages"
            );
            match backup.step(1024) {
                Ok(StepResult::Done) => break,
                Ok(StepResult::Busy | StepResult::Locked) => {
                    std::thread::sleep(Duration::from_millis(10))
                }
                Ok(StepResult::More) => {}
                Ok(_) => bail!("unsupported SQLite backup status"),
                Err(error) => bail!(
                    "SQLite stream failed: {error}; {}",
                    channel.error.as_deref().unwrap_or("no channel diagnostic")
                ),
            }
        }
    }
    output.close().map_err(|(_, error)| error)?;
    input.close().map_err(|(_, error)| error)?;
    channel.finish()
}

pub fn stream(source: &Path, destination: &str, receiver: &str, output: &str) -> Result<Streamed> {
    transfer::validate_ssh_target(destination)?;
    ensure!(
        !receiver.is_empty()
            && !receiver.chars().any(char::is_control)
            && !output.is_empty()
            && !output.chars().any(char::is_control),
        "receiver/output must be explicit safe paths"
    );
    let input = Connection::open_with_flags(source, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    ensure!(
        fs::symlink_metadata(source)?.is_file(),
        "stream source must be a regular file"
    );
    input.execute_batch("BEGIN")?;
    let pages =
        u64::try_from(input.query_row("PRAGMA page_count", [], |row| row.get::<_, i64>(0))?)?;
    let page_size =
        u64::try_from(input.query_row("PRAGMA page_size", [], |row| row.get::<_, i64>(0))?)?;
    let max_bytes = pages
        .checked_mul(page_size)
        .and_then(|size| size.checked_add(1024 * 1024))
        .context("SQLite size overflow")?;
    let quote = |value: &str| format!("'{}'", value.replace('\'', "'\\''"));
    let command = format!(
        "{} transfer receive-sqlite --output {} --max-bytes {}",
        quote(receiver),
        quote(output),
        max_bytes
    );
    backup_input(
        source,
        input,
        Command::new("ssh").args([
            "-o",
            "BatchMode=yes",
            "-o",
            "ConnectTimeout=8",
            "-o",
            "ServerAliveInterval=15",
            "-o",
            "ServerAliveCountMax=3",
            "--",
            destination,
            &command,
        ]),
    )
}
