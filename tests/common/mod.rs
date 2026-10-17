use std::{
    fs::File,
    io::Read,
    os::fd::{FromRawFd, OwnedFd},
    process::{Command, Stdio},
};

/// Run `command` with stdout on a pseudo-terminal, so Parcela prints human output.
/// Color is off through NO_COLOR. Returns the exit code and stdout with `\n` line ends.
pub fn on_terminal(mut command: Command) -> (Option<i32>, String) {
    let (mut primary, mut secondary) = (0, 0);
    // SAFETY: openpty only writes the two descriptors; the name and settings are optional.
    let opened = unsafe {
        libc::openpty(
            &mut primary,
            &mut secondary,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    assert_eq!(opened, 0, "openpty failed");
    // SAFETY: openpty returned two fresh descriptors that nothing else owns.
    let (mut reader, writer) =
        unsafe { (File::from_raw_fd(primary), OwnedFd::from_raw_fd(secondary)) };
    let mut child = command
        .env("NO_COLOR", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::from(writer))
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    drop(command);
    let mut bytes = Vec::new();
    let mut buffer = [0; 4096];
    // The terminal reports EOF or EIO once the child closes its end.
    while let Ok(read @ 1..) = reader.read(&mut buffer) {
        bytes.extend_from_slice(&buffer[..read]);
    }
    let status = child.wait().unwrap();
    (
        status.code(),
        String::from_utf8(bytes).unwrap().replace("\r\n", "\n"),
    )
}
