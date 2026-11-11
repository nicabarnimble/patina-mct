ARG RUST_IMAGE=rust:1.96.0-bookworm@sha256:5e2214abe154fe26e39f64488952e5c991eeed1d6d6da7cc8381ae83927f0cfc
ARG RUNTIME_IMAGE=debian:bookworm-slim@sha256:734a72b3d6142a71993b4bc580ca01d8ca687f77a2c422d96a961d3376b179bc
ARG PRODUCT_VERSION

FROM --platform=linux/amd64 ${RUST_IMAGE} AS build
ARG SOURCE_COMMIT
ARG SOURCE_TREE
ARG LOCK_SHA256
ARG PRODUCT_VERSION
WORKDIR /src
COPY . .
RUN test -n "${SOURCE_COMMIT}" \
 && test -n "${SOURCE_TREE}" \
 && test -n "${LOCK_SHA256}" \
 && test -n "${PRODUCT_VERSION}" \
 && test "$(git rev-parse HEAD)" = "${SOURCE_COMMIT}" \
 && test "$(git rev-parse HEAD^{tree})" = "${SOURCE_TREE}" \
 && test -z "$(git status --porcelain --untracked-files=no)" \
 && test "$(sha256sum Cargo.lock | cut -d' ' -f1)" = "${LOCK_SHA256}"
RUN cargo fetch --locked --target x86_64-unknown-linux-gnu
RUN cargo build --release --locked --offline --target x86_64-unknown-linux-gnu \
      -p mct-daemon --bin mct-daemon \
 && test "$(target/x86_64-unknown-linux-gnu/release/mct-daemon version)" = "mct-daemon ${PRODUCT_VERSION}" \
 && install -D -m 0555 target/x86_64-unknown-linux-gnu/release/mct-daemon /out/mct-daemon \
 && printf '{"source_commit":"%s","source_tree":"%s","cargo_lock_sha256":"%s","rust":"1.96.0","target":"x86_64-unknown-linux-gnu","product":"mct-daemon","version":"%s"}\n' \
      "${SOURCE_COMMIT}" "${SOURCE_TREE}" "${LOCK_SHA256}" "${PRODUCT_VERSION}" > /out/source.json

FROM build AS smoke-build
RUN cargo test --release --locked --offline --target x86_64-unknown-linux-gnu \
      -p mct-daemon --test nic131_repair \
      nic131_repair_governed_peers_exchange_receiver_issued_proof_and_call --no-run \
 && test_exe=$(find target/x86_64-unknown-linux-gnu/release/deps -maxdepth 1 -type f \
      -name 'nic131_repair-*' -perm -0100 | head -n 1) \
 && test -n "$test_exe" \
 && install -D -m 0555 "$test_exe" /out/mct-container-smoke

FROM --platform=linux/amd64 ${RUNTIME_IMAGE} AS mother
ARG SOURCE_COMMIT
ARG SOURCE_TREE
ARG LOCK_SHA256
ARG PRODUCT_VERSION
LABEL org.opencontainers.image.title="MCT Mother" \
      org.opencontainers.image.description="Governed MCT Mother runtime" \
      org.opencontainers.image.source="https://github.com/nicabarnimble/patina-mct" \
      org.opencontainers.image.revision="${SOURCE_COMMIT}" \
      org.opencontainers.image.version="${PRODUCT_VERSION}" \
      io.patina.mct.source-tree="${SOURCE_TREE}" \
      io.patina.mct.cargo-lock-sha256="${LOCK_SHA256}"
RUN printf 'mct:x:10001:\n' >> /etc/group \
 && printf 'mct:x:10001:10001:MCT Mother:/var/lib/mct:/usr/sbin/nologin\n' >> /etc/passwd \
 && mkdir -p /var/lib/mct /var/lib/mct-smoke /usr/share/mct \
 && chown 10001:10001 /var/lib/mct /var/lib/mct-smoke \
 && chmod 0700 /var/lib/mct /var/lib/mct-smoke
COPY --from=build /out/mct-daemon /usr/local/bin/mct-daemon
COPY --from=build /out/source.json /usr/share/mct/source.json
COPY --chmod=0555 scripts/container-entrypoint.sh /usr/local/bin/mct-container-entrypoint
USER 10001:10001
WORKDIR /var/lib/mct
VOLUME ["/var/lib/mct"]
EXPOSE 11204/udp
HEALTHCHECK --interval=10s --timeout=3s --start-period=15s --retries=6 \
  CMD ["/usr/local/bin/mct-daemon", "status", "--uds", "/var/lib/mct/control.sock", "--json"]
ENTRYPOINT ["/usr/local/bin/mct-container-entrypoint"]

FROM mother AS smoke
COPY --from=smoke-build /out/mct-container-smoke /usr/local/bin/mct-container-smoke
COPY crates/mct-daemon/tests/fixtures/mct-test-echo-0.1.0/mct-test-echo.wasm \
     /src/crates/mct-daemon/tests/fixtures/mct-test-echo-0.1.0/mct-test-echo.wasm
ENV NIC131_DAEMON=/usr/local/bin/mct-daemon \
    TMPDIR=/var/lib/mct-smoke \
    RUST_BACKTRACE=1
WORKDIR /var/lib/mct-smoke
ENTRYPOINT ["/usr/local/bin/mct-container-smoke", "--exact", "nic131_repair_governed_peers_exchange_receiver_issued_proof_and_call", "--nocapture"]
