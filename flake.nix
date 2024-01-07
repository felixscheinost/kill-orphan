{
  description = "Build a cargo project";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";

    crane = {
      url = "github:ipetkov/crane";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    flake-utils.url = "github:numtide/flake-utils";

    advisory-db = {
      url = "github:rustsec/advisory-db";
      flake = false;
    };

    rust-overlay.url = "github:oxalica/rust-overlay";
  };

  outputs =
    { self, nixpkgs, crane, flake-utils, advisory-db, rust-overlay, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs { inherit system overlays; };

        inherit (pkgs) lib;

        # Using that instead of fenix as for some reason fenix didn't work properly with VSCode and rust-analyzer
        my-rust = pkgs.rust-bin.selectLatestNightlyWith (toolchain:
          toolchain.default.override { extensions = [ "rust-src" ]; });

        craneLib = (crane.mkLib pkgs).overrideToolchain my-rust;

        src = craneLib.cleanCargoSource (craneLib.path ./.);

        commonArgs = {
          inherit src;
          strictDeps = true;

          buildInputs = [
            # Add additional build inputs here
          ] ++ lib.optionals pkgs.stdenv.isDarwin [
            # Additional darwin specific inputs can be set here
            pkgs.libiconv
            pkgs.darwin.IOKit
          ];

          # Additional environment variables can be set directly
          # MY_CUSTOM_VAR = "some value";
        };

        # Build *just* the cargo dependencies, so we can reuse
        # all of that work (e.g. via cachix) when running in CI
        cargoArtifacts = craneLib.buildDepsOnly commonArgs;

        # Build the actual crate itself, reusing the dependency
        # artifacts from above.
        my-crate =
          craneLib.buildPackage (commonArgs // { inherit cargoArtifacts; });

      in {
        checks = {
          # Build the crate as part of `nix flake check` for convenience
          inherit my-crate;

          # Run clippy (and deny all warnings) on the crate source,
          # again, resuing the dependency artifacts from above.
          #
          # Note that this is done as a separate derivation so that
          # we can block the CI if there are issues here, but not
          # prevent downstream consumers from building our crate by itself.
          my-crate-clippy = craneLib.cargoClippy (commonArgs // {
            inherit cargoArtifacts;
            cargoClippyExtraArgs = "--all-targets -- --deny warnings";
          });

          my-crate-doc =
            craneLib.cargoDoc (commonArgs // { inherit cargoArtifacts; });

          # Check formatting
          my-crate-fmt = craneLib.cargoFmt { inherit src; };

          # Audit dependencies
          my-crate-audit = craneLib.cargoAudit { inherit src advisory-db; };

          # Audit licenses
          # my-crate-deny = craneLib.cargoDeny { inherit src; };

          # Run tests with cargo-nextest
          # Consider setting `doCheck = false` on `my-crate` if you do not want
          # the tests to run twice
          # my-crate-nextest = craneLib.cargoNextest (commonArgs // {
          #   inherit cargoArtifacts;
          #   partitions = 1;
          #   partitionType = "count";
          # });
        };

        packages = {
          default = my-crate;
        } // lib.optionalAttrs (!pkgs.stdenv.isDarwin) {
          my-crate-llvm-coverage =
            craneLib.cargoLlvmCov (commonArgs // { inherit cargoArtifacts; });
        };

        apps.default = flake-utils.lib.mkApp { drv = my-crate; };

        devShells.default = craneLib.devShell {
          # Inherit inputs from checks.
          checks = self.checks.${system};

          # Additional dev-shell environment variables can be set directly
          # MY_CUSTOM_DEVELOPMENT_VAR = "something else";

          # Extra inputs can be added here; cargo and rustc are provided by default.
          packages = [ ];
        };
      });
}
