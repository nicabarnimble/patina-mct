# Governed profile (NIC-126)

Entry points: `mct-daemon passkey-admin prepare|sign-day|apply|send-iroh` and `mct-daemon serve` on a passkey-rooted root.

| Proof | What it shows |
| --- | --- |
| `cargo test -p mct-daemon --test governed_boot` | CLI genesis and a sorted day-key lease. A Child is staged and approved, and a peer admitted, only by signed commands. One `serve` process becomes ready, projects `config.json`, completes a call, applies a signed `/peers/revoke` over `mct/admin/1` from the printed ticket, and refuses a Unix-socket approval. |
| `cargo test -p mct-daemon --bin mct-daemon done_c2` | A tampered `config.json` approval grants nothing; the day-key chain admits the route and directory grant; revocations deny. Eight Unix-socket routes and one offline path refuse, with the ledger bytes unchanged. |
| `cargo test -p mct-observation --lib passkey_admin_tests::governed` | Governed facts refuse an open-ended grant or peer, an escaping root, an unpinned artifact, and a wrong scope before any append, and they replay identically. |
| `cargo test -p mct-daemon --test mother_restart done_c7_unkeyed` | D-40: an unkeyed call ID never relaunches after SIGKILL and SQLite rollback. |

Run the tests with `umask 077` and a canonical `TMPDIR`. The macOS default `TMPDIR` sits under the `/var` symlink, and private-storage checks refuse it.

Not proven here: the browser 1Password signature (a software ES256 key stands in) and any Pi run. Record both as manual and `not-run`.
