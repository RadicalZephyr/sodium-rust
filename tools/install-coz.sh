#!/usr/bin/env bash
#
# Install the Coz causal profiler, for profiling coz-driver.
#
#   tools/install-coz.sh          # install the pinned version
#   COZ_VERSION=0.2.6 tools/install-coz.sh   # override (checksum check is skipped)
#
# Coz is not needed to build or test this repository, so it is installed on
# demand rather than as part of any setup. Once it is in place:
#
#   cargo build --release -p coz-driver
#   coz run --- ./target/release/coz-driver
#   coz plot -i profile.coz
#
# The release ships an unstripped libcoz.so, so expect ~171 MB installed.
#
set -euo pipefail

COZ_VERSION="${COZ_VERSION:-0.2.5}"
ARCH="$(dpkg --print-architecture)"
URL="https://github.com/plasma-umass/coz/releases/download/v${COZ_VERSION}/coz_${COZ_VERSION}_${ARCH}.deb"

# sha256 of the pinned release assets. Unset for any other version, in which
# case the download is installed unverified and the script says so.
declare -A CHECKSUMS=(
  ["0.2.5:amd64"]="175dee6cc759f92913a761cc5f2fbbb667451c676c3183295f254a8ed0dfae39"
  ["0.2.5:arm64"]="00ac124e149e28b953a776e0e58cc8fabda2f0994a98b7574cb4a2af0cb7d085"
)
EXPECTED="${CHECKSUMS[${COZ_VERSION}:${ARCH}]:-}"

SUDO=""
if [ "$(id -u)" -ne 0 ]; then
  SUDO="sudo"
fi

# Match on status as well as version: a removed-but-not-purged package still
# reports its version to dpkg-query while its files are gone.
state="$(dpkg-query -W -f='${db:Status-Status} ${Version}' coz 2>/dev/null || true)"
if [ "$state" = "installed ${COZ_VERSION}" ]; then
  echo "coz ${COZ_VERSION} already installed"
  exit 0
fi

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

echo "Downloading coz ${COZ_VERSION} (${ARCH})..."
curl -fsSL --retry 3 --retry-delay 2 -o "$tmp/coz.deb" "$URL"

if [ -n "$EXPECTED" ]; then
  actual="$(sha256sum "$tmp/coz.deb" | cut -d' ' -f1)"
  if [ "$actual" != "$EXPECTED" ]; then
    echo "checksum mismatch for $URL" >&2
    echo "  expected $EXPECTED" >&2
    echo "  actual   $actual" >&2
    exit 1
  fi
else
  echo "warning: no pinned checksum for ${COZ_VERSION}:${ARCH}, installing unverified" >&2
fi

$SUDO dpkg -i "$tmp/coz.deb"

# `coz --version` reports "unknown", so ask dpkg instead.
dpkg-query -W -f='installed coz ${Version}\n' coz
