with import <nixpkgs> {};
mkShell {
  name = "gst-coquitts";
  NIX_CFLAGS_COMPILE = lib.optionals stdenv.isDarwin [
    "-I${lib.getDev libcxx}/include/c++/v1"
  ];
  buildInputs = [
    cargo
    cargo-c
    cmake
    pkg-config
    python3
    git
    glib
    gst_all_1.gstreamer
    gst_all_1.gst-plugins-base
    gst_all_1.gst-plugins-good
  ];
  shellHook = ''
    export OPENBLAS_NUM_THREADS=1
    SOURCE_DATE_EPOCH=$(date +%s)
    if test ! -d .venv ; then
      python -m venv .venv
    fi
    source ./.venv/bin/activate
    pip install -U pip tts
    export PYTHONPATH=$(pwd)/.venv/${python3.sitePackages}:$PYTHONPATH
  '';
}
