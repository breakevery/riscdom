#!/usr/bin/env sh
# Package the relay, the connection layer's server (v1.0 M7b-1; relay-only since v1.0 M8-4c).
#
#   scripts/pack.sh [--output-dir <dir>]
#
# It builds the release binary and assembles **one** archive in the host's own platform format:
#
#   riscdom-relay-<version>-<platform>.tar.gz    the connection layer's server
#
# What the package holds is [docs/server-distribution.md](../docs/server-distribution.md)'s business;
# this script only builds what that document says. The **single-node control plane**
# (`riscdom-server`) left this repository in v1.0 M8-4a and is packaged in its own repository
# (<https://github.com/breakevery/riscdom-server>), so this packer no longer builds it and no longer
# copies a front end into a package.
#
# It is the **unix twin** of `scripts/pack.ps1` (the same split `gate` and `commit`
# keep, so the Windows shape has a native implementation rather than a
# dependency on `zip`): this half writes `.tar.gz` with the system `tar`, the other writes `.zip` with
# PowerShell's own `Compress-Archive`.
#
# It never tags, never publishes, and never copies a data
# directory or a credential into a package: the relay's key is minted by the program on
# its first start, where it belongs.
set -eu

cd "$(cd "$(dirname "$0")/.." && pwd)"

usage() {
  echo "usage: scripts/pack.sh [--output-dir <dir>]"
  echo ""
  echo "  --output-dir <dir>  where the archive goes (default: target/dist)"
}

output_dir="target/dist"
while [ $# -gt 0 ]; do
  case "$1" in
    --output-dir)
      [ $# -ge 2 ] || { echo "pack: --output-dir needs a value"; exit 2; }
      output_dir="$2"
      shift 2
      ;;
    -h | --help)
      usage
      exit 0
      ;;
    *)
      echo "pack: unknown option $1"
      usage
      exit 2
      ;;
  esac
done

# The version comes from the workspace, which is where `[workspace.package] version` lives: the one
# place a release bumps, so a package cannot disagree with the binary inside it.
version=$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -n 1)
if [ -z "$version" ]; then
  echo "pack: cannot read the version from Cargo.toml"
  exit 1
fi

case "$(uname -s)" in
  Linux) platform="linux-$(uname -m)" ;;
  Darwin) platform="macos-$(uname -m)" ;;
  *) platform="unix-$(uname -m)" ;;
esac

echo "==> cargo build --release (riscdom-relay)"
cargo build --release -p net --bin riscdom-relay

staging="$output_dir/.staging-$version"
rm -rf "$staging"
mkdir -p "$staging" "$output_dir"

# ---- the connection layer's server ----------------------------------------
relay_name="riscdom-relay-$version-$platform"
relay_dir="$staging/$relay_name"
mkdir -p "$relay_dir/examples"
cp "target/release/riscdom-relay" "$relay_dir/"
cp net/README.md "$relay_dir/README.md"
printf '{\n  "schema_version": 1,\n  "peers": []\n}\n' >"$relay_dir/examples/peers.example.json"
printf '{\n  "schema_version": 1,\n  "rooms": []\n}\n' >"$relay_dir/examples/rooms.example.json"

echo "==> tar.gz"
tar -czf "$output_dir/$relay_name.tar.gz" -C "$staging" "$relay_name"
rm -rf "$staging"

echo "pack: ok"
archive="$output_dir/$relay_name.tar.gz"
bytes=$(wc -c <"$archive" | tr -d ' ')
echo "  $archive ($bytes bytes)"
