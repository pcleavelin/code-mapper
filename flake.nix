{
  description = "codemap: named, annotated code paths over a symbol graph";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";

  outputs =
    { self, nixpkgs }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "x86_64-darwin"
        "aarch64-darwin"
      ];
      forAllSystems = f: nixpkgs.lib.genAttrs systems (system: f nixpkgs.legacyPackages.${system});

      runtimeLibs =
        pkgs:
        pkgs.lib.optionals pkgs.stdenv.hostPlatform.isLinux (
          with pkgs;
          [
            wayland
            libxkbcommon
            vulkan-loader
            libGL
            libx11
            libxcursor
            libxi
            libxrandr
          ]
        );

      hostDrivers = pkgs: ''
        if [ ! -e /run/opengl-driver ]; then
          export VK_DRIVER_FILES="''${VK_DRIVER_FILES:-${pkgs.mesa}/share/vulkan/icd.d}"
          export __EGL_VENDOR_LIBRARY_FILENAMES="''${__EGL_VENDOR_LIBRARY_FILENAMES:-${pkgs.mesa}/share/glvnd/egl_vendor.d/50_mesa.json}"
        fi
      '';
    in
    {
      packages = forAllSystems (
        pkgs:
        let
          libPath = pkgs.lib.makeLibraryPath (runtimeLibs pkgs);
        in
        {
          default = pkgs.rustPlatform.buildRustPackage {
            pname = "codemap";
            version = "0.1.0";
            src = pkgs.lib.cleanSourceWith {
              src = ./.;
              filter =
                path: type:
                let
                  base = baseNameOf path;
                in
                !(builtins.elem base [
                  "target"
                  ".codemap-cache"
                  ".jj"
                ]);
            };
            cargoLock.lockFile = ./Cargo.lock;
            cargoBuildFlags = [
              "--package"
              "codemap"
            ];
            doCheck = false;
            nativeBuildInputs = pkgs.lib.optionals pkgs.stdenv.hostPlatform.isLinux [ pkgs.makeWrapper ];
            postInstall = pkgs.lib.optionalString pkgs.stdenv.hostPlatform.isLinux ''
              wrapProgram $out/bin/codemap --suffix LD_LIBRARY_PATH : ${libPath} \
                --run ${pkgs.lib.escapeShellArg (hostDrivers pkgs)}
            '';
            meta.mainProgram = "codemap";
          };
        }
      );

      devShells = forAllSystems (
        pkgs:
        let
          headless = pkgs.writeShellApplication {
            name = "headless";
            runtimeInputs = [
              pkgs.weston
              pkgs.inotify-tools
            ];
            text = ''
              export XDG_RUNTIME_DIR="''${XDG_RUNTIME_DIR:-/run/user/$(id -u)}"
              socket=codemap-test
              if ! [ -S "$XDG_RUNTIME_DIR/$socket" ]; then
                weston --backend=headless --renderer=pixman --shell=kiosk --socket="$socket" \
                  --width=1600 --height=1000 >"$XDG_RUNTIME_DIR/weston-$socket.log" 2>&1 &
                until [ -S "$XDG_RUNTIME_DIR/$socket" ]; do
                  inotifywait -qq -t 1 -e create "$XDG_RUNTIME_DIR" || true
                done
              fi
              export WAYLAND_DISPLAY="$socket"
              export VK_DRIVER_FILES=${pkgs.mesa}/share/vulkan/icd.d/lvp_icd.${pkgs.stdenv.hostPlatform.uname.processor}.json
              unset DISPLAY
              exec "$@"
            '';
          };
        in
        {
          default = pkgs.mkShell {
            packages =
              with pkgs;
              [
                cargo
                rustc
                clippy
                rustfmt
                rust-analyzer
                pkg-config
                jujutsu
              ]
              ++ lib.optionals stdenv.hostPlatform.isLinux [
                headless
              ];
            LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath (runtimeLibs pkgs);
            RUST_SRC_PATH = "${pkgs.rustPlatform.rustLibSrc}";
            shellHook = pkgs.lib.optionalString pkgs.stdenv.hostPlatform.isLinux (hostDrivers pkgs);
          };
        }
      );

      formatter = forAllSystems (pkgs: pkgs.nixfmt-tree);
    };
}
