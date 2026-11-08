use std::io::{Read, Seek, SeekFrom, Write};

use mct_kernel::Timestamp;

// Two reads inside `execute_passkey_command_with_clock` cannot be split from
// outside the process. `NIC131_CLOCK_FILE` supplies them in order. An unset,
// empty, or unreadable file leaves the production clock in place.
pub(crate) fn pop() -> Option<Timestamp> {
    let path = std::env::var_os("NIC131_CLOCK_FILE")?;
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _guard = LOCK.lock().ok()?;
    let mut file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .ok()?;
    let mut text = String::new();
    file.read_to_string(&mut text).ok()?;
    let mut lines = text.lines();
    let first = lines.next()?.trim();
    if first.is_empty() {
        return None;
    }
    let stamp = Timestamp::new(first).ok()?;
    let rest = lines.fold(String::new(), |mut acc, line| {
        acc.push_str(line);
        acc.push('\n');
        acc
    });
    file.seek(SeekFrom::Start(0)).ok()?;
    file.set_len(0).ok()?;
    file.write_all(rest.as_bytes()).ok()?;
    Some(stamp)
}
