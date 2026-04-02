{
  description = "OpenDataSync - Wikidata + Wikicommons + Overpass API";
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };
  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let pkgs = nixpkgs.legacyPackages.${system}; in {
        packages = {
          opendatasync = pkgs.rustPlatform.buildRustPackage {
            pname = "opendatasync";
            version = "0.1.0";
            src = ./.;
            cargoLock.lockFile = ./Cargo.lock;
            nativeBuildInputs = [ pkgs.pkg-config ];
            buildInputs = [ pkgs.openssl ];
          };
          default = self.packages.${system}.opendatasync;
        };
        apps.default = {
          type = "app";
          program = "${self.packages.${system}.opendatasync}/bin/opendatasync";
        };
        devShells.default = pkgs.mkShell {
          buildInputs = with pkgs; [ cargo rustc rust-analyzer pkg-config openssl ];
        };
      }
    );
}
