use super::*;
use std::{
    io::{Read, Write},
    os::unix::{fs::PermissionsExt, net::UnixListener},
};

#[test]
fn cli_status_and_mutation_reject_oversized_replies_without_fallback() {
    for mutation in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("control.sock");
        let listener = UnixListener::bind(&path).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut head = Vec::new();
            while !head.ends_with(b"\r\n\r\n") {
                let mut byte = [0];
                stream.read_exact(&mut byte).unwrap();
                head.push(byte[0]);
                assert!(head.len() <= 4096);
            }
            if mutation {
                let mut body = [0; 2];
                stream.read_exact(&mut body).unwrap();
            }
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n",
                mct_daemon::control_transport::CONTROL_RESPONSE_BYTES + 1
            )
            .unwrap();
        });
        let error = if mutation {
            try_resident_control_mutation(&path, "/test", b"{}").unwrap_err()
        } else {
            query_resident_status(&path).unwrap_err()
        };
        assert!(format!("{error:#}").contains("byte cap"), "{error:#}");
        server.join().unwrap();
    }
}

#[test]
fn cli_never_sends_to_unverified_socket_or_treats_it_as_absent() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("control.sock");
    let listener = UnixListener::bind(&path).unwrap();
    listener.set_nonblocking(true).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o666)).unwrap();
    assert!(try_resident_control_mutation(&path, "/test", b"privileged").is_err());
    assert!(query_resident_status(&path).is_err());
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
}
