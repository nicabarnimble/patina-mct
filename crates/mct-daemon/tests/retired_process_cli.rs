#![cfg(unix)]

use std::{fs, os::unix::fs::PermissionsExt, process::Command};

fn retired_input_cannot_spawn(subcommand: &[&str]) {
    let dir = tempfile::tempdir().unwrap();
    let marker = dir.path().join("spawned");
    let executable = dir.path().join("retired-child.sh");
    fs::write(
        &executable,
        format!("#!/bin/sh\nprintf spawned > '{}'\n", marker.display()),
    )
    .unwrap();
    let mut permissions = fs::metadata(&executable).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&executable, permissions).unwrap();

    let args = subcommand
        .iter()
        .map(|arg| {
            if *arg == "$EXECUTABLE" {
                executable.display().to_string()
            } else {
                (*arg).to_owned()
            }
        })
        .collect::<Vec<_>>();
    let output = Command::new(env!("CARGO_BIN_EXE_mct-daemon"))
        .args(args)
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(!marker.exists(), "retired CLI input spawned an executable");
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("unknown"), "{stderr}");
}

#[test]
fn retired_process_commands_are_unknown_and_have_no_spawn_effect() {
    retired_input_cannot_spawn(&["process", "call", "$EXECUTABLE"]);
    retired_input_cannot_spawn(&["iroh", "serve-process", "$EXECUTABLE"]);
}

#[test]
fn help_exposes_no_process_child_command() {
    let output = Command::new(env!("CARGO_BIN_EXE_mct-daemon"))
        .arg("--help")
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(!stdout.contains("process call"));
    assert!(!stdout.contains("serve-process"));
}
