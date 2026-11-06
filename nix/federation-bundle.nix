{ artifacts }:
let
  source = builtins.getFlake "path:${toString ../.}";
  pkgs = source.inputs.nixpkgs.legacyPackages.${builtins.currentSystem};
in
pkgs.runCommand "mct-federation-test-bundle" { src = artifacts; } ''
  mkdir -p "$out/bin"
  install -m555 "$src/mct-daemon" "$out/bin/mct-daemon"
  install -m555 "$src/federation_host" "$out/bin/mct-federation-host"
  install -m444 "$src/provenance.json" "$out/provenance.json"
  mkdir -p "$out/share"
  install -m444 "$src/mct-repo-report.wasm" "$out/share/mct-repo-report.wasm"
  install -m444 "$src/child.toml" "$out/share/child.toml"
  mkdir -p "$out/share/snapshots/a" "$out/share/snapshots/b"
  install -m444 "$src/snapshots/a/Cargo.lock" "$out/share/snapshots/a/Cargo.lock"
  install -m444 "$src/snapshots/b/Cargo.lock" "$out/share/snapshots/b/Cargo.lock"
  mkdir -p "$out/libexec"
  ln -s ${pkgs.nftables}/bin/nft "$out/libexec/nft"
  ln -s ${pkgs.bpftools}/bin/bpftool "$out/libexec/bpftool"
''
