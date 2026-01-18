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
            pkgs.rust-bin.stable.latest.default
          ];

          env.PDFIUM_DYNAMIC_LIB_PATH = "${pkgs.pdfium-binaries}/lib";
        };
      });

      packages = forAllSystems (
        pkgs:
        let
          pname = "pdf-images";
          version = "0.1.2";
        in
        rec {
          default = pkgs.rustPlatform.buildRustPackage {
            inherit pname version;

            src = pkgs.lib.cleanSourceWith {
              src = self;
              filter =
                filePath: type:
                let
                  baseName = baseNameOf filePath;
                in
                !builtins.elem baseName [
                  "flake.nix"
                  "flake.lock"
                  "README.md"
                  ".helix"
                  ".envrc"
                  ".gitignore"
                ];
            };

            cargoLock.lockFile = ./Cargo.lock;

            nativeBuildInputs = [ pkgs.makeWrapper ];

            postInstall = ''
              wrapProgram $out/bin/pdf-images \
                --set PDFIUM_DYNAMIC_LIB_PATH "${pkgs.pdfium-binaries}/lib"
            '';
          };

          docker = pkgs.dockerTools.buildLayeredImage {
            name = pname;
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

          deploy = pkgs.writeShellScriptBin "deploy" ''
            ${pkgs.skopeo}/bin/skopeo --insecure-policy copy docker-archive:${docker} docker://docker.io/frectonz/${pname}:${version} --dest-creds="frectonz:$ACCESS_TOKEN"
            ${pkgs.skopeo}/bin/skopeo --insecure-policy copy docker://docker.io/frectonz/${pname}:${version} docker://docker.io/frectonz/${pname}:latest --dest-creds="frectonz:$ACCESS_TOKEN"
          '';
        }
      );

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
