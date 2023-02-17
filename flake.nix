{
	inputs = {
		nixpkgs.url = "nixpkgs/nixos-unstable";
		flake-utils.url = "github:numtide/flake-utils";

		rust-overlay = {
			url = "github:oxalica/rust-overlay";
			inputs.nixpkgs.follows = "nixpkgs";
			inputs.flake-utils.follows = "flake-utils";
		};
	};

	outputs = { self, nixpkgs, flake-utils, rust-overlay, ... }:
		flake-utils.lib.eachDefaultSystem (system:
			let
				pkgs = import nixpkgs {
					inherit system;
					overlays = [ (import rust-overlay) ];
				};
			in with pkgs; {
				devShells.default = mkShell {
					buildInputs = [
						(rust-bin.selectLatestNightlyWith (toolchain: toolchain.default.override {
							extensions = [
								"rustc"
								"rust-src"
								"rust-docs"
								"rust-std"
								"cargo"
								"clippy"
								"rust-analyzer"
								"miri"
							];
						}))

						cargo-expand
					];
				};

				devShells.stable = mkShell {
					buildInputs = [ rust-bin.stable.latest.default ];
				};
			});
}
