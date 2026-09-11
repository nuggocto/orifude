{
  description = "Orifude, an offline folding and ink puzzle game";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    rust-overlay.inputs.nixpkgs.follows = "nixpkgs";
  };

  outputs = { self, nixpkgs, rust-overlay }:
    let
      systems = [ "x86_64-linux" "aarch64-linux" ];
      forEachSystem = nixpkgs.lib.genAttrs systems;
    in {
      packages = forEachSystem (system:
        let
          pkgs = import nixpkgs {
            inherit system;
            overlays = [ rust-overlay.overlays.default ];
          };
          toolchain = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
          rustPlatform = pkgs.makeRustPlatform {
            cargo = toolchain;
            rustc = toolchain;
          };
          manifest = builtins.fromTOML (builtins.readFile ./Cargo.toml);
        in rec {
          orifude = rustPlatform.buildRustPackage {
            pname = manifest.package.name;
            version = manifest.package.version;
            src = pkgs.lib.cleanSourceWith {
              src = ./.;
              filter = path: type:
                pkgs.lib.cleanSourceFilter path type
                && !(builtins.elem (builtins.baseNameOf path) [ "target" "result" ]);
            };
            cargoLock.lockFile = ./Cargo.lock;

            nativeCheckInputs = [ pkgs.util-linux ];
            doInstallCheck = true;
            # Exercise the installed production binary with the existing player journey.
            # Test-only features are enabled after installation, so they cannot ship.
            installCheckPhase = ''
              runHook preInstallCheck
              export ORIFUDE_ARTIFACT_BINARY="$out/bin/orifude"
              cargo test --locked --offline --release \
                --target ${pkgs.stdenv.hostPlatform.rust.rustcTarget} \
                --features isolated-test-paths --test terminal_pty \
                packaged_binary_preserves_the_complete_player_journey -- --ignored --exact
              runHook postInstallCheck
            '';

            meta = {
              description = manifest.package.description;
              homepage = manifest.package.homepage;
              license = pkgs.lib.licenses.asl20;
              mainProgram = "orifude";
              platforms = systems;
            };
          };
          default = orifude;
        });

      apps = forEachSystem (system: {
        default = {
          type = "app";
          program = "${self.packages.${system}.orifude}/bin/orifude";
          meta.description = "Play Orifude";
        };
      });

      checks = forEachSystem (system: {
        package = self.packages.${system}.orifude;
      });
    };
}
