{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs = {
        nixpkgs.follows = "nixpkgs";
      };
    };
  };

  outputs =
    {
      self,
      nixpkgs,
      rust-overlay,
    }:
    let
      forAllSystems =
        fn:
        let
          systems = [
            "x86_64-linux"
            "aarch64-darwin"
          ];
          overlays = [ (import rust-overlay) ];
        in
        nixpkgs.lib.genAttrs systems (
          system:
          fn (
            import nixpkgs {
              inherit system overlays;
            }
          )
        );
    in
    {
      devShells = forAllSystems (pkgs: {
        default = pkgs.mkShell {
          buildInputs = [
            pkgs.xh
            pkgs.rust-analyzer
            pkgs.pdfium-binaries
            pkgs.rust-bin.stable.latest.default
          ];

          env.PDFIUM_DYNAMIC_LIB_PATH = "${pkgs.pdfium-binaries}/lib";
        };
      });

      packages = forAllSystems (pkgs: {
        default = pkgs.rustPlatform.buildRustPackage {
          pname = "pdf-images";
          version = "0.1.0";
          src = self;

          cargoLock.lockFile = ./Cargo.lock;

          nativeBuildInputs = [ pkgs.makeWrapper ];

          postInstall = ''
            wrapProgram $out/bin/pdf-images \
              --set PDFIUM_DYNAMIC_LIB_PATH "${pkgs.pdfium-binaries}/lib"
          '';
        };

        docker = pkgs.dockerTools.buildLayeredImage {
          name = "pdf-images";
          tag = "latest";
          contents = [
            default
            pkgs.cacert
          ];
          config = {
            Cmd = [ "${default}/bin/pdf-images" ];
            Env = [
              "SSL_CERT_FILE=${pkgs.cacert}/etc/ssl/certs/ca-bundle.crt"
            ];
          };
        };
      });

      formatter = forAllSystems (
        pkgs:
        pkgs.treefmt.withConfig {
          runtimeInputs = [ pkgs.nixfmt-rfc-style ];
          settings = {
            on-unmatched = "info";
            formatter.nixfmt = {
              command = "nixfmt";
              includes = [ "*.nix" ];
            };
          };
        }
      );
    };
}
