#!/usr/bin/env sh
# Package the two deployable programs (v1.0 M7b-1).
#
#   scripts/pack.sh [--output-dir <dir>] [--skip-ui-build]
#
# It builds the release binaries and assembles **two** archives in the host's own platform format:
#
#   riscdom-server-<version>-<platform>.tar.gz   the single-node control plane
#   riscdom-relay-<version>-<platform>.tar.gz    the connection layer's server
#
# What each package holds is [docs/server-distribution.md](../docs/server-distribution.md)'s business;
# this script only builds what that document says. It is the **unix twin** of `scripts/pack.ps1` (the
# same split `gate` and `commit` keep, so the Windows shape has a native implementation rather than a
# dependency on `zip`): this half writes `.tar.gz` with the system `tar`, the other writes `.zip` with
# PowerShell's own `Compress-Archive`.
#
# It never runs in CI (that is a later batch), never tags, never publishes, and never copies a data
# directory or a credential into a package: the token and the node key are minted by the programs on
# their first start, where they belong.
set -eu

cd "$(cd "$(dirname "$0")/.." && pwd)"

usage() {
  echo "usage: scripts/pack.sh [--output-dir <dir>] [--skip-ui-build]"
  echo ""
  echo "  --output-dir <dir>  where the archives go (default: target/dist)"
  echo "  --skip-ui-build     reuse the front end already built at ui/dist/app"
}

output_dir="target/dist"
skip_ui=0
while [ $# -gt 0 ]; do
  case "$1" in
    --output-dir)
      [ $# -ge 2 ] || { echo "pack: --output-dir needs a value"; exit 2; }
      output_dir="$2"
      shift 2
      ;;
    --skip-ui-build)
      skip_ui=1
      shift
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
# place a release bumps, so a package cannot disagree with the binaries inside it.
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

echo "==> cargo build --release (riscdom-server, riscdom-relay)"
cargo build --release -p server --bin riscdom-server
cargo build --release -p net --bin riscdom-relay

web_root="ui/dist/app"
if [ "$skip_ui" -eq 0 ]; then
  echo "==> npm run build (ui)"
  # `npm run build`, not `npm ci`: the gate assumes the frontend dependencies are installed, and a
  # packaging script that reached the network would be a different thing than the one the project runs.
  (cd ui && npm run build)
fi
if [ ! -d "$web_root" ]; then
  echo "pack: the built front end is not at $web_root -- build it first (drop --skip-ui-build)"
  exit 1
fi

staging="$output_dir/.staging-$version"
rm -rf "$staging"
mkdir -p "$staging" "$output_dir"

# ---- the single-node control plane ----------------------------------------
server_name="riscdom-server-$version-$platform"
server_dir="$staging/$server_name"
mkdir -p "$server_dir/web"
cp "target/release/riscdom-server" "$server_dir/"
cp -R "$web_root/." "$server_dir/web/"
cp server/README.md "$server_dir/README.md"
printf '{\n  "version": 2\n}\n' >"$server_dir/settings.example.json"

# ---- the connection layer's server ----------------------------------------
relay_name="riscdom-relay-$version-$platform"
relay_dir="$staging/$relay_name"
mkdir -p "$relay_dir/examples"
cp "target/release/riscdom-relay" "$relay_dir/"
cp net/README.md "$relay_dir/README.md"
printf '{\n  "schema_version": 1,\n  "peers": []\n}\n' >"$relay_dir/examples/peers.example.json"
printf '{\n  "schema_version": 1,\n  "rooms": []\n}\n' >"$relay_dir/examples/rooms.example.json"

echo "==> tar.gz"
tar -czf "$output_dir/$server_name.tar.gz" -C "$staging" "$server_name"
tar -czf "$output_dir/$relay_name.tar.gz" -C "$staging" "$relay_name"
rm -rf "$staging"

echo "pack: ok"
for archive in "$output_dir/$server_name.tar.gz" "$output_dir/$relay_name.tar.gz"; do
  bytes=$(wc -c <"$archive" | tr -d ' ')
  echo "  $archive ($bytes bytes)"
done
