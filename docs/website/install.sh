#!/bin/sh
# Installs the latest Parcela release into ~/.local/bin, plus Caddy and Process Compose when no
# supported version is on PATH. Run with --help for options.
# Served at https://parcela.davafons.com/install.sh from github.com/davafons/parcela.
set -eu

# The versions Parcela's live tests run against.
CADDY_VERSION="${CADDY_VERSION:-2.11.7}"
PROCESS_COMPOSE_VERSION="${PROCESS_COMPOSE_VERSION:-1.122.0}"
BIN="$HOME/.local/bin"

usage() {
  cat <<EOF
Usage: install.sh [--nightly]

Installs Parcela into ~/.local/bin, plus Caddy 2 and Process Compose 1 when
no supported version is on PATH.

Options:
  --nightly    Install the newest build of main instead of the latest release
  -h, --help   Show this help

Environment:
  PARCELA_SKIP_BACKENDS=1    Install only Parcela
  CADDY_VERSION              Caddy version to install (default $CADDY_VERSION)
  PROCESS_COMPOSE_VERSION    Process Compose version to install (default $PROCESS_COMPOSE_VERSION)
EOF
}

# digest ALGORITHM FILE prints the file's hex digest.
digest() {
  if command -v "sha$1sum" >/dev/null 2>&1; then
    "sha$1sum" "$2" | cut -d' ' -f1
  else
    shasum -a "$1" "$2" | cut -d' ' -f1
  fi
}

download() {
  curl --proto '=https' --tlsv1.2 -fsSL "$1" -o "$2"
}

# install_parcela RELEASE_URL installs the build for this machine, checked against its checksum.
install_parcela() {
  case "$(uname -s)-$(uname -m)" in
    Darwin-arm64) target=aarch64-apple-darwin ;;
    Darwin-x86_64) target=x86_64-apple-darwin ;;
    Linux-x86_64) target=x86_64-unknown-linux-gnu ;;
    Linux-aarch64 | Linux-arm64) target=aarch64-unknown-linux-gnu ;;
    *) echo "no Parcela build for $(uname -s) $(uname -m); see https://parcela.davafons.com/docs/" >&2; exit 1 ;;
  esac
  release=$1
  work=$(mktemp -d)
  # The nightly assets are replaced in place, so a download can pair a new tarball with an old
  # checksum. Try once more before giving up.
  for attempt in 1 2; do
    download "$release/parcela-$target.tar.gz" "$work/parcela.tar.gz"
    download "$release/parcela-$target.tar.gz.sha256" "$work/sha256"
    [ "$(digest 256 "$work/parcela.tar.gz")" = "$(cut -d' ' -f1 "$work/sha256")" ] && break
    if [ "$attempt" = 2 ]; then
      echo "parcela: checksum mismatch; nothing installed" >&2
      rm -rf "$work"
      exit 1
    fi
    echo "parcela: checksum mismatch; retrying" >&2
    sleep 10
  done
  tar -xzf "$work/parcela.tar.gz" -C "$work"
  mkdir -p "$BIN"
  install -m 755 "$work/parcela-$target/parcela" "$BIN/parcela"
  rm -rf "$work"
  echo "installed $("$BIN/parcela" --version) to $BIN/parcela"
}

# fetch NAME URL CHECKSUMS_URL ARCHIVE ALGORITHM downloads ARCHIVE, checks it against the
# release's checksum file and installs the NAME binary from it into $BIN.
fetch() {
  work=$(mktemp -d)
  download "$2/$4" "$work/$4"
  download "$3" "$work/checksums"
  expected=$(grep " $4\$" "$work/checksums" | cut -d' ' -f1)
  if [ -z "$expected" ] || [ "$(digest "$5" "$work/$4")" != "$expected" ]; then
    echo "$1: checksum mismatch for $4; nothing installed" >&2
    rm -rf "$work"
    return 1
  fi
  tar -xzf "$work/$4" -C "$work" "$1"
  mkdir -p "$BIN"
  install -m 755 "$work/$1" "$BIN/$1"
  rm -rf "$work"
  echo "installed $1 to $BIN/$1"
  wrote="$wrote ~/.local/bin/$1"
}

# supported NAME BINARY succeeds when BINARY is a version Parcela runs, matching its own checks.
supported() {
  version=$("$2" version 2>/dev/null) || return 1
  case "$1" in
    caddy) case "$version" in v2.*) return 0 ;; esac ;;
    process-compose) case "$version" in *v1.*) return 0 ;; esac ;;
  esac
  return 1
}

# backend NAME NEED installs NAME into $BIN unless a supported one is on PATH or already there.
# A found binary of another version stays put; the pinned one goes into $BIN beside it.
backend() {
  found=$(command -v "$1" 2>/dev/null || true)
  [ -z "$found" ] && [ -x "$BIN/$1" ] && found="$BIN/$1"
  [ -n "$found" ] && supported "$1" "$found" && return 0
  if [ -n "$found" ] && [ "$found" != "$BIN/$1" ] && [ -x "$BIN/$1" ] && supported "$1" "$BIN/$1"; then
    echo "$1: unsupported version at $found; using ~/.local/bin/$1"
    installed="$installed $1"
    return 0
  fi
  [ -n "$found" ] && echo "$1: unsupported version at $found; installing $2 into ~/.local/bin"
  if [ "$1" = caddy ]; then
    release="https://github.com/caddyserver/caddy/releases/download/v$CADDY_VERSION"
    fetch caddy "$release" "$release/caddy_${CADDY_VERSION}_checksums.txt" \
      "caddy_${CADDY_VERSION}_${caddy_os}_${arch}.tar.gz" 512
  else
    release="https://github.com/F1bonacc1/process-compose/releases/download/v$PROCESS_COMPOSE_VERSION"
    fetch process-compose "$release" "$release/process-compose_checksums.txt" \
      "process-compose_${pc_os}_${arch}.tar.gz" 256
  fi
  installed="$installed $1"
}

# path_hint prints how to put $BIN on PATH, or ahead of an older backend that shadows it.
path_hint() {
  case ":$PATH:" in
    *":$BIN:"*)
      shadowed=
      for name in $installed; do
        found=$(command -v "$name")
        case "$found $shadowed " in
          "$BIN/$name "* | *" ${found%/*} "*) ;;
          *) shadowed="$shadowed ${found%/*}" ;;
        esac
      done
      [ -z "$shadowed" ] && return 0
      echo
      echo "Put ~/.local/bin ahead of$shadowed on PATH:"
      ;;
    *)
      echo
      echo "Add ~/.local/bin to PATH:"
      ;;
  esac
  case "${SHELL##*/}" in
    fish) echo "  fish_add_path --move ~/.local/bin" ;;
    zsh) echo "  echo 'export PATH=\"\$HOME/.local/bin:\$PATH\"' >> ~/.zshrc" ;;
    bash) echo "  echo 'export PATH=\"\$HOME/.local/bin:\$PATH\"' >> ~/.bashrc" ;;
    *) echo "  export PATH=\"\$HOME/.local/bin:\$PATH\"" ;;
  esac
}

# uninstall_hint names every file this script wrote.
uninstall_hint() {
  echo
  echo "To uninstall: rm$wrote"
}

main() {
  nightly=
  for arg in "$@"; do
    case "$arg" in
      --nightly) nightly=1 ;;
      -h | --help) usage; exit 0 ;;
      *) echo "unknown option: $arg" >&2; usage >&2; exit 2 ;;
    esac
  done

  if [ -n "$nightly" ]; then
    install_parcela https://github.com/davafons/parcela/releases/download/nightly
  else
    install_parcela https://github.com/davafons/parcela/releases/latest/download
  fi
  wrote=" ~/.local/bin/parcela"
  installed=

  if [ "${PARCELA_SKIP_BACKENDS:-}" != 1 ]; then
    case "$(uname -s)-$(uname -m)" in
      Darwin-x86_64) caddy_os=mac pc_os=darwin arch=amd64 ;;
      Darwin-arm64) caddy_os=mac pc_os=darwin arch=arm64 ;;
      Linux-x86_64 | Linux-amd64) caddy_os=linux pc_os=linux arch=amd64 ;;
      Linux-aarch64 | Linux-arm64) caddy_os=linux pc_os=linux arch=arm64 ;;
      *) caddy_os= ;;
    esac
    if [ -n "$caddy_os" ]; then
      backend caddy "Caddy $CADDY_VERSION"
      backend process-compose "Process Compose $PROCESS_COMPOSE_VERSION"
    else
      echo "Install Caddy 2 and Process Compose 1 yourself; run parcela doctor to check."
    fi
  fi

  path_hint
  echo
  echo "Next, in your project: parcela init, then parcela up"
  uninstall_hint
}

main "$@"
