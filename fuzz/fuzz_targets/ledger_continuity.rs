#![no_main]

use libfuzzer_sys::fuzz_target;
use mct_kernel::{MctObservation, ObservationId, ObservationKind, Timestamp, TraceId};
use mct_observation::{
    JsonlObservationLedger, load_verified_ledger_replay, private_fs, verify_ledger_streaming,
};
use std::{path::PathBuf, sync::OnceLock};

struct Fixture {
    path: PathBuf,
    baseline: Vec<u8>,
    head: String,
}

fn fixture() -> &'static Fixture {
    static FIXTURE: OnceLock<Fixture> = OnceLock::new();
    FIXTURE.get_or_init(|| {
        let root = std::env::temp_dir()
            .canonicalize()
            .unwrap()
            .join(private_fs::nonce_name("mct-ledger-fuzz").unwrap());
        private_fs::ensure_directory(&root).unwrap();
        let path = root.join("ledger");
        let mut writer = JsonlObservationLedger::open(&path, "fuzz-ledger", "fuzz-mother").unwrap();
        for n in 0..3 {
            writer
                .append_before_effect(
                    MctObservation::informational(
                        ObservationId::new(format!("obs-{n}")).unwrap(),
                        Timestamp::new("2026-10-25T00:00:00Z").unwrap(),
                        ObservationKind::LifecycleTransitionRecorded,
                        TraceId::new("trace").unwrap(),
                        "continuity fixture",
                    ),
                    "2026-10-25T00:00:00Z",
                )
                .unwrap();
        }
        drop(writer);
        let head = load_verified_ledger_replay(&path, "fuzz-ledger", "fuzz-mother")
            .unwrap()
            .head
            .unwrap()
            .entry_hash;
        eprintln!("ledger continuity fixture: {}", path.display());
        Fixture {
            baseline: std::fs::read(&path).unwrap(),
            path,
            head,
        }
    })
}

fuzz_target!(|data: &[u8]| {
    if data.is_empty() || data.len() > 64 * 1024 {
        return;
    }
    let f = fixture();
    let candidate = match data[0] % 4 {
        0 => data[1..].to_vec(),
        1 => f.baseline[..data.len() % f.baseline.len()].to_vec(),
        2 => [f.baseline.as_slice(), &data[1..]].concat(),
        _ => {
            // Different raw bytes, identical historical typed representation.
            let spaces = " ".repeat((data[0] / 4) as usize + 1);
            String::from_utf8(f.baseline.clone())
                .unwrap()
                .replace(',', &format!(",{spaces}"))
                .into_bytes()
        }
    };
    private_fs::write_atomic(&f.path, &candidate).unwrap();
    // This full verifier consults the known anchor but does not publish a newer
    // cache head, so each iteration independently tests the same baseline.
    if let Ok(verified) = verify_ledger_streaming(&f.path, "fuzz-ledger", "fuzz-mother") {
        assert!(verified.head.unwrap().local_sequence >= 2);
        let reader =
            JsonlObservationLedger::open_read_only(&f.path, "fuzz-ledger", "fuzz-mother").unwrap();
        assert_eq!(reader.entries().unwrap()[2].entry_hash, f.head);
    }
    assert_eq!(
        std::fs::read(&f.path).unwrap(),
        candidate,
        "verification must never repair evidence"
    );
});
