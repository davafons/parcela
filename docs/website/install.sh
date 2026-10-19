#!/bin/sh
# Installs the latest Parcela release into ~/.local/bin, plus Caddy and Process Compose when they
# are not on PATH. `--nightly` installs the newest build of main instead. Set
# PARCELA_SKIP_BACKENDS=1 to install only Parcela.
# Served at https://parcela.davafons.com/install.sh from github.com/davafons/parcela.
set -eu

# The versions Parcela's live tests run against.
CADDY_VERSION="${CADDY_VERSION:-2.11.7}"
PROCESS_COMPOSE_VERSION="${PROCESS_COMPOSE_VERSION:-1.122.0}"
BIN="$HOME/.local/bin"

nightly=
for arg in "$@"; do
  [ "$arg" = --nightly ] && nightly=1
done

# digest ALGORITHM FILE prints the file's hex digest.
digest() {
  if command -v "sha$1sum" >/dev/null 2>&1; then
    "sha$1sum" "$2" | cut -d' ' -f1
  else
    shasum -a "$1" "$2" | cut -d' ' -f1
  fi
}

if [ -n "$nightly" ]; then
  case "$(uname -s)-$(uname -m)" in
    Darwin-arm64) target=aarch64-apple-darwin ;;
    Darwin-x86_64) target=x86_64-apple-darwin ;;
    Linux-x86_64) target=x86_64-unknown-linux-gnu ;;
    Linux-aarch64 | Linux-arm64) target=aarch64-unknown-linux-gnu ;;
    *) echo "no nightly build for $(uname -s) $(uname -m)" >&2; exit 1 ;;
  esac
  release=https://github.com/davafons/parcela/releases/download/nightly
  work=$(mktemp -d)
  curl --proto '=https' --tlsv1.2 -fsSL "$release/parcela-$target.tar.gz" -o "$work/parcela.tar.gz"
  curl --proto '=https' --tlsv1.2 -fsSL "$release/parcela-$target.tar.gz.sha256" -o "$work/sha256"
  if [ "$(digest 256 "$work/parcela.tar.gz")" != "$(cut -d' ' -f1 "$work/sha256")" ]; then
    echo "parcela: checksum mismatch for the nightly build; nothing installed" >&2
    rm -rf "$work"
    exit 1
  fi
  tar -xzf "$work/parcela.tar.gz" -C "$work"
  mkdir -p "$BIN"
  install -m 755 "$work/parcela-$target/parcela" "$BIN/parcela"
  rm -rf "$work"
  echo "installed $("$BIN/parcela" --version) to $BIN/parcela"
else
  curl --proto '=https' --tlsv1.2 -LsSf \
    https://github.com/davafons/parcela/releases/latest/download/parcela-installer.sh | sh -s -- "$@"
fi

[ "${PARCELA_SKIP_BACKENDS:-}" = 1 ] && exit 0

case "$(uname -s)" in
  Darwin) caddy_os=mac pc_os=darwin ;;
  Linux) caddy_os=linux pc_os=linux ;;
  *) echo "Install Caddy 2 and Process Compose 1 yourself; run parcela doctor to check."; exit 0 ;;
esac
case "$(uname -m)" in
  x86_64 | amd64) arch=amd64 ;;
  arm64 | aarch64) arch=arm64 ;;
  *) echo "Install Caddy 2 and Process Compose 1 yourself; run parcela doctor to check."; exit 0 ;;
esac

# fetch NAME URL CHECKSUMS_URL ARCHIVE ALGORITHM downloads ARCHIVE, checks it against the
# release's checksum file and installs the NAME binary from it into $BIN.
fetch() {
  work=$(mktemp -d)
  curl --proto '=https' --tlsv1.2 -fsSL "$2/$4" -o "$work/$4"
  curl --proto '=https' --tlsv1.2 -fsSL "$3" -o "$work/checksums"
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
}

if ! command -v caddy >/dev/null 2>&1 && [ ! -x "$BIN/caddy" ]; then
  release="https://github.com/caddyserver/caddy/releases/download/v$CADDY_VERSION"
  fetch caddy "$release" "$release/caddy_${CADDY_VERSION}_checksums.txt" \
    "caddy_${CADDY_VERSION}_${caddy_os}_${arch}.tar.gz" 512
fi
if ! command -v process-compose >/dev/null 2>&1 && [ ! -x "$BIN/process-compose" ]; then
  release="https://github.com/F1bonacc1/process-compose/releases/download/v$PROCESS_COMPOSE_VERSION"
  fetch process-compose "$release" "$release/process-compose_checksums.txt" \
    "process-compose_${pc_os}_${arch}.tar.gz" 256
fi

echo
echo "Next, in your project: parcela init, then parcela up"
