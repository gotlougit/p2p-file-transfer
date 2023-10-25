{
  outputs = { self, nixpkgs }:
    let
      system = "x86_64-linux";
      pkgs = import nixpkgs { inherit system; };
      mkShell = pkgs.mkShell.override { stdenv = pkgs.stdenvAdapters.useMoldLinker pkgs.stdenv; };
    in
    {
      devShells.${system}.default = mkShell {
        name = "rustdev";
        shellHook = ''
          export CARGO_HOME="$(realpath ./.localcargo)"
        '';
        buildInputs = [
          pkgs.pkg-config
          pkgs.rustc
          pkgs.cargo
          pkgs.rustfmt
          pkgs.clippy
          pkgs.rust-analyzer
          pkgs.cargo-rr
          pkgs.tree-sitter
          pkgs.gdb
        ];
      };
    };
}

