//! C3 hostile fixture Children at the actual Wasmtime boundary (baseline evidence).
use super::*;

const DOUBLE: &str = "patina:demo/control@0.1.0.double";

fn fixture_child(dir: &tempfile::TempDir, name: &str, wat: &str) -> MctLoadedChild {
    let path = dir.path().join(format!("{name}.component.wasm"));
    fs::write(&path, wat::parse_str(wat).unwrap()).unwrap();
    loaded_typed_child(path, vec![DOUBLE.into()])
}

fn invoke_with_wasi_enabled(
    child: &MctLoadedChild,
) -> Result<MctWitComponentInvocationReport, MctWasmComponentRuntimeError> {
    runtime().invoke_authorized_child_wit_export_with_host_adapters(
        authorized(&child.wasm_path),
        child,
        &typed_call("double"),
        &serde_json::json!([7]),
        wasi_host_adapters(),
        ids(),
    )
}

fn assert_refused_at_instantiate(name: &str, wat: &str, import: &str) {
    let dir = tempfile::tempdir().unwrap();
    let child = fixture_child(&dir, name, wat);
    match invoke_with_wasi_enabled(&child) {
        Err(MctWasmComponentRuntimeError::UnsupportedWitHostImport { import_name, .. }) => {
            assert_eq!(import_name, import)
        }
        other => panic!("{name} was not refused at instantiate: {other:?}"),
    }
}

#[test]
fn hostile_tcp_socket_import_is_refused_even_with_wasi_enabled() {
    assert_refused_at_instantiate(
        "sockets-tcp",
        include_str!("../../../tests/fixtures/hostile/sockets-tcp.wat"),
        "wasi:sockets/tcp@0.2.3",
    );
}

#[test]
fn hostile_name_lookup_import_is_refused_even_with_wasi_enabled() {
    assert_refused_at_instantiate(
        "sockets-ip-name-lookup",
        include_str!("../../../tests/fixtures/hostile/sockets-ip-name-lookup.wat"),
        "wasi:sockets/ip-name-lookup@0.2.3",
    );
}

#[test]
fn hostile_outgoing_http_import_is_refused_even_with_wasi_enabled() {
    assert_refused_at_instantiate(
        "http-outgoing-handler",
        include_str!("../../../tests/fixtures/hostile/http-outgoing-handler.wat"),
        "wasi:http/outgoing-handler@0.2.3",
    );
}

#[test]
fn hostile_control_granted_wasi_import_still_runs() {
    let dir = tempfile::tempdir().unwrap();
    let child = loaded_typed_child(
        typed_wasi_filesystem_import_component_path(&dir),
        vec![DOUBLE.into()],
    );
    let report = invoke_with_wasi_enabled(&child).unwrap();
    assert_eq!(report.result.outcome, ResultOutcome::Success);
}
