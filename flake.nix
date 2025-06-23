{
  description = "A Nix flake for the tale/rootbeer system configuration tool";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

    flake-parts = {
      url = "github:hercules-ci/flake-parts";
      inputs.nixpkgs-lib.follows = "nixpkgs";
    };

    systems = {
      url = "github:nix-systems/default";
    };
  };

  outputs = inputs: inputs.flake-parts.lib.mkFlake { inherit inputs; } {
    systems = import inputs.systems;
    perSystem = { self', lib, pkgs, system, ... }: {
      packages = {
        rootbeer = let
          # NOTE: as per <./meson.build>
          version = "0.0.1";
          source-with-meson-deps = pkgs.stdenv.mkDerivation {
            pname = "rootbeer-deps";
            inherit version;

            src = lib.cleanSource ./.;

            nativeBuildInputs = [
              pkgs.pkg-config
              pkgs.meson
              pkgs.cmake
              pkgs.git
              pkgs.cacert
              pkgs.ninja
            ];

            outputHashAlgo = "sha256";
            outputHashMode = "recursive";
            outputHash = "sha256-DdbXF0LdCptpY5zDApOUDHonDlMjRl7ut9m/46Lj+74=";

            phases = [
              "installPhase"
            ];

            installPhase = ''
              # Copy over the raw source in the output, since we'll do the vendoring work there
              mkdir -p $out
              cd $out
              cp --no-preserve=mode -r $src/. .

              # Set the SSL certificate file for network access
              export NIX_SSL_CERT_FILE=${pkgs.cacert}/etc/ssl/certs/ca-bundle.crt

              # Download the subprojects the `meson` way (using networking, hence the FOD)
              ${pkgs.lib.getExe pkgs.meson} subprojects download \
                cjson luajit

              # Clean up `git` metadata from the downloaded dependencies
              find subprojects -type d -name .git -prune -execdir rm -r {} +
            '';
          };
        in pkgs.stdenv.mkDerivation {
          pname = "rootbeer";
          inherit version;

          src = source-with-meson-deps;

          nativeBuildInputs = [
            pkgs.pkg-config
            pkgs.ninja
            pkgs.meson
            pkgs.autoPatchelfHook
          ];

          buildInputs = [
          ];

          postInstall = ''
            # Manually install the main executable, left in the `src` directory, for some reason
            mkdir -p $out/bin
            cp ./src/rootbeer_cli/rb $out/bin/

            # Install provided libraries
            # TODO: not sure what the structure of those should be for other consumers
            mkdir -p $out/lib/rootbeer
            cp -r ./src/librootbeer/. $out/lib/rootbeer/
          '';

          meta = with pkgs.lib; {
            description = "A tool to deterministically manage your system using Lua";
            homepage = "Deterministically manage your system using Lua!";
            # NOTE: as per <./meson.build>
            license = licenses.mit;
            platforms = platforms.all;
            maintainers = with maintainers; [ ];
          };
        };

        default = self'.packages.rootbeer;
      };

      devShells = {
        default = pkgs.mkShell {
          packages = self'.packages.rootbeer.buildInputs ++ self'.packages.rootbeer.nativeBuildInputs;
        };
      };
    };
  };
}
