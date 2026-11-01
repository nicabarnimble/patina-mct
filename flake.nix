{
  description = "Native Linux ARM64 MCT admin Mother and isolated hardware gate";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    rust-overlay.inputs.nixpkgs.follows = "nixpkgs";
  };

  outputs = { self, nixpkgs, rust-overlay }:
    let
      system = "aarch64-linux";
      pkgs = import nixpkgs { inherit system; overlays = [ rust-overlay.overlays.default ]; };
      toolchain = pkgs.rust-bin.stable."1.96.0".minimal;
      rustPlatform = pkgs.makeRustPlatform { cargo = toolchain; rustc = toolchain; };
      mother = rustPlatform.buildRustPackage {
        pname = "mct-admin-mother";
        version = "0.2.0-nic53";
        src = pkgs.lib.fileset.toSource {
          root = ./.;
          fileset = pkgs.lib.fileset.unions [ ./Cargo.toml ./Cargo.lock ./crates ./fuzz/corpus ./scripts/fixtures ];
        };
        cargoLock = {
          lockFile = ./Cargo.lock;
          outputHashes."patina-sdk-0.22.0" = "sha256-xX8+cAj2D22frDw6kR7y2JWnGg6veX7PJv+fmqz1ttw=";
        };
        cargoBuildFlags = [ "-p" "mct-daemon" "--bin" "mct-daemon" "-p" "mct-install-probe" "--bin" "mct-install-probe" ];
        nativeBuildInputs = [ pkgs.pkg-config ];
        CARGO_BUILD_JOBS = "2";
        # The hardware gate tests the installed programs, outside the build sandbox.
        doCheck = false;
        meta.platforms = [ system ];
      };
      python = pkgs.python3.withPackages (p: [ p.cryptography ]);
      service = pkgs.writeShellApplication {
        name = "mother-admin-service";
        runtimeInputs = [ pkgs.nix pkgs.util-linux ];
        text = ''
          exec ${python}/bin/python3 ${./scripts}/mother-admin-service.py "$@" \
            --binary ${mother}/bin/mct-daemon \
            --unit-template ${./nix/mother-admin.service.in} --entrypoint "$0"
        '';
      };
      smoke = pkgs.writeShellApplication {
        name = "mother-install-smoke";
        runtimeInputs = [ pkgs.nix pkgs.util-linux pkgs.coreutils ];
        text = ''
          exec ${python}/bin/python3 ${./scripts}/mother-install-smoke.py \
            --binary ${mother}/bin/mct-daemon \
            --probe ${mother}/bin/mct-install-probe \
            --installer ${service}/bin/mother-admin-service \
            --unit-template ${./nix/mother-admin.service.in} "$@"
        '';
      };
    in {
      packages.${system} = { default = mother; inherit mother; mother-install-smoke = smoke; mother-admin-service = service; };
      apps.${system} = {
        mother-install-smoke = { type = "app"; program = "${smoke}/bin/mother-install-smoke"; };
        mother-admin-service = { type = "app"; program = "${service}/bin/mother-admin-service"; };
      };
    };
}
