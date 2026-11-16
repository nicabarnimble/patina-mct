use super::*;
fn req(bytes: &[u8]) -> String {
    format!("{{\"sha256\":\"{:x}\"}}", Sha256::digest(bytes))
}
#[test]
fn real_lockfile() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let bytes = std::fs::read(root.join("Cargo.lock")).unwrap();
    let a = inspect_in(&root, &req(&bytes)).unwrap();
    assert_eq!(a, inspect_in(&root, &req(&bytes)).unwrap());
    let json: serde_json::Value = serde_json::from_str(&a).unwrap();
    assert!(!json["selected"]["wasmtime"].as_array().unwrap().is_empty());
}
#[test]
fn counts_and_duplicates() {
    let b = b"version = 4\n[[package]]\nname = \"a\"\nversion = \"1\"\nsource = \"registry+x\"\n[[package]]\nname = \"a\"\nversion = \"2\"\nsource = \"git+x\"\n[[package]]\nname = \"b\"\nversion = \"1\"\n";
    let result = inspect_bytes(&req(b), b).unwrap();
    let v: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert_eq!(v["registry_count"], 1);
    assert_eq!(v["git_count"], 1);
    assert_eq!(v["local_count"], 1);
    assert_eq!(v["packages"]["a"], serde_json::json!(["1", "2"]));
    assert_eq!(v["packages"]["b"], serde_json::json!(["1"]));
    assert_eq!(v["duplicates"]["a"], serde_json::json!(["1", "2"]));
}
#[test]
fn rejects_invalid() {
    let b = b"version = 4\n[[package]]\nname = \"a\"\nversion = \"1\"\nsource = \"other+x\"\n";
    assert_eq!(inspect_bytes(&req(b), b), Err("unknown source"));
    let uppercase = format!("{{\"sha256\":\"{:X}\"}}", Sha256::digest(b));
    assert_eq!(inspect_bytes(&uppercase, b), Err("invalid digest"));
    assert!(inspect_bytes("{\"sha256\":\"no\"}", b).is_err());
    let malformed = b"version = 4\n[[package]]\nname = \"a\"\n";
    assert!(inspect_bytes(&req(malformed), malformed).is_err());
    let big = alloc::vec![b'x'; INPUT_LIMIT + 1];
    assert_eq!(inspect_bytes(&req(&big), &big), Err("input too large"));
    assert!(request("{\"sha256\":\"abc\",\"path\":\"../x\"}").is_err());
}
#[test]
fn rejects_large_envelope() {
    let mut b = String::from("version = 4\n");
    for i in 0..1000 {
        for v in ["1", "2"] {
            b.push_str(&format!(
                "[[package]]\nname = \"package-{i:04}-long-name\"\nversion = \"{v}\"\n"
            ));
        }
    }
    assert_eq!(
        inspect_bytes(&req(b.as_bytes()), b.as_bytes()),
        Err("output too large")
    );
}
