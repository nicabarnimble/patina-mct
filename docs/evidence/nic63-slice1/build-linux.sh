#!/bin/bash
set -euo pipefail
export CARGO_TARGET_DIR=/home/pi/mct-nic63-slice1/target
export CARGO_BUILD_JOBS=2
export CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0
cd /home/pi/mct-nic63-slice1/source
rustup toolchain install 1.96.0 --profile minimal
rustc +1.96.0 --version
cargo +1.96.0 --version
cargo +1.96.0 test --locked -p mct-daemon --lib --bin mct-daemon --test federation_fleet --test mother_restart --no-run --message-format=json > /evidence/linux-daemon-build.jsonl
cargo +1.96.0 test --locked -p mct-kernel -p mct-observation --lib --no-run --message-format=json > /evidence/linux-libraries-build.jsonl
printf '0\n' > /evidence/linux-build.exit
