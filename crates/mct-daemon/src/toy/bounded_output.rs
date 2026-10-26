//! Bound native Git output while it is read, not after Command::output allocates.
use std::{
    io::{self, Read},
    process::{Command, Output, Stdio},
    sync::mpsc,
};
const STDOUT_LIMIT: usize = 256 * 1024;
const STDERR_LIMIT: usize = 64 * 1024;

pub(super) fn run(command: &mut Command) -> io::Result<Output> {
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let stdout = child.stdout.take().expect("piped stdout");
    let stderr = child.stderr.take().expect("piped stderr");
    std::thread::scope(|scope| {
        let (tx, rx) = mpsc::channel();
        let spawn_reader = |reader: Box<dyn Read + Send>, limit, is_stdout| {
            let tx = tx.clone();
            scope.spawn(move || {
                let mut bytes = Vec::new();
                let result = reader
                    .take(limit as u64 + 1)
                    .read_to_end(&mut bytes)
                    .and_then(|_| {
                        if bytes.len() > limit {
                            Err(io::Error::other("toy output byte limit exceeded"))
                        } else {
                            Ok(bytes)
                        }
                    });
                let _ = tx.send((is_stdout, result));
            });
        };
        spawn_reader(Box::new(stdout), STDOUT_LIMIT, true);
        spawn_reader(Box::new(stderr), STDERR_LIMIT, false);
        drop(tx);
        let mut out = Vec::new();
        let mut err = Vec::new();
        let mut failure = None;
        for (is_stdout, result) in rx {
            match result {
                Ok(bytes) if is_stdout => out = bytes,
                Ok(bytes) => err = bytes,
                Err(error) => {
                    let _ = child.kill();
                    failure.get_or_insert(error);
                }
            }
        }
        let status = child.wait()?;
        if let Some(error) = failure {
            return Err(error);
        }
        Ok(Output {
            status,
            stdout: out,
            stderr: err,
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_output_overflow_fails_while_small_output_passes() {
        let output = run(Command::new("printf").arg("bounded")).unwrap();
        assert_eq!(output.stdout, b"bounded");
        // Finite generator; no shell, racing mutation or unbounded allocation.
        let result = run(Command::new("head").args(["-c", "262145", "/dev/zero"]));
        assert!(result.is_err());
    }
}
