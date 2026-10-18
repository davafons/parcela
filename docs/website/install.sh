#!/bin/sh
# Installs the latest Parcela release into ~/.local/bin.
# Served at https://parcela.davafons.com/install.sh from github.com/davafons/parcela.
set -eu

curl --proto '=https' --tlsv1.2 -LsSf \
  https://github.com/davafons/parcela/releases/latest/download/parcela-installer.sh | sh -s -- "$@"

for tool in caddy process-compose; do
  command -v "$tool" >/dev/null 2>&1 || missing="${missing:-}${missing:+ and }$tool"
done
if [ -n "${missing:-}" ]; then
  echo
  echo "Parcela also needs $missing. Run \`parcela doctor\` for install links."
fi
