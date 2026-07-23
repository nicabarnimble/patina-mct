# MCT 0.2.0 release install and upgrade guide

MCT 0.2.0 is a pre-GA `aarch64-apple-darwin` release. Ad-hoc signing proves bundle consistency, not Apple-notarized publisher identity. Obtain the archive and both sidecars through an operator-controlled channel.

## Verify and extract

From a trusted checkout of the matching source revision:

```bash
archive=/absolute/path/mct-daemon-v0.2.0-aarch64-apple-darwin.tar.gz
./scripts/verify-release-artifact.sh "$archive"
```

The verifier checks the external SHA-256 and BLAKE3 identities, hostile-archive bounds, exact layout and internal checksums, release metadata, and the extracted bundle with `codesign --verify --strict`.

Extract only after verification. The executable is:

```text
mct-daemon-v0.2.0-aarch64-apple-darwin/
  payload/mct-daemon.app/Contents/MacOS/mct-daemon
```

## Clean install

Use an absolute path to the extracted packaged executable:

```bash
MCT=/absolute/path/mct-daemon-v0.2.0-aarch64-apple-darwin/payload/mct-daemon.app/Contents/MacOS/mct-daemon
"$MCT" install --executable "$MCT"
"$MCT" start
"$MCT" status --json
```

The default service root is `~/.mct`; the managed launchd policy is `~/Library/LaunchAgents/io.patina.mct.mother.plist`. Installation does not grant Child or Toy authority.

## Evidence-informed upgrade

Run upgrade through the exact executable bound by the current supervisor record:

```bash
CURRENT=/absolute/path/to/current/mct-daemon
CANDIDATE=/absolute/path/to/mct-daemon-v0.2.0-aarch64-apple-darwin.tar.gz
"$CURRENT" upgrade "$CANDIDATE"
```

Upgrade acquires and verifies the archive before displaying its exact release notes, provenance, target, executable digests, acquisition observations, and plan. At the prompt, type the complete candidate identity:

```text
sha256:<64 lowercase hex characters from the candidate archive>
```

A filename, version, `yes`, EOF, prior approval, or another digest is denied before stop or replacement. For non-interactive operation, the same exact identity is required:

```bash
"$CURRENT" upgrade "$CANDIDATE" \
  --expected-digest sha256:<candidate-archive-digest> \
  --approve-artifact sha256:<candidate-archive-digest> \
  --json
```

After approval, upgrade composes the existing clean stop, `install --replace`, and start paths. It waits at most `MCT_UPGRADE_POST_VERIFY_DEADLINE_SECONDS` (30 seconds) for healthy/ready status with the candidate version, successor supervisor revision, and executable digest.

## Failed post-verification and rollback

Upgrade never rolls back automatically. A failure records `upgrade_failed`, retains all immutable daemon releases, and prints the exact service root plus rollback shape. Diagnose first:

```bash
"$CURRENT" status --json
launchctl print "gui/$(id -u)/io.patina.mct.mother"
tail -n 100 ~/.mct/logs/mother.stderr.log
```

Select a prior executable only from its retained verified immutable release directory, then explicitly compose rollback:

```bash
CURRENT=/absolute/path/to/the/current-invoker
PRIOR=~/.mct/releases/sha256/<prior-archive-digest>/mct-daemon-v<version>-aarch64-apple-darwin/payload/mct-daemon.app/Contents/MacOS/mct-daemon

"$CURRENT" stop
"$CURRENT" install --replace --executable "$PRIOR"
"$PRIOR" start
"$PRIOR" status --json
```

A lower semantic version is not accepted by guided `upgrade`; rollback is this explicit retained-evidence lifecycle operation.

## Release smoke exclusivity

`scripts/release-local.sh smoke` uses the same fixed launchd label as production and therefore refuses if a production resident is loaded. The script never stops production. An operator running smoke on a daily-driver machine must stop production explicitly and restart it afterward.

Smoke does not touch production supervisor files: it snapshots the default record and plist and requires byte-for-byte postflight equality. Its alternate plist path exists only inside a separately feature-built smoke harness; no production CLI flag, environment variable, or config field can select it.

## Unsupported and deferred paths

MCT 0.2.0 does not provide network release discovery/fetch, background update checks, notarized identity, Homebrew, Linux/systemd supervision, Linux signing, JVM SDK distribution, or launcher/interface orchestration. A local archive is evidence input, never update authority by itself.
