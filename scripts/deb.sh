#!/usr/bin/env bash
# Build the Irida .deb. The maintainer comes from DEB_MAINTAINER at build
# time, so no address is ever committed. Prints the path of the .deb.
set -euo pipefail

usage="set DEB_MAINTAINER to 'Your Name <you AT example.com>' with a real at-sign"
if [ -z "${DEB_MAINTAINER:-}" ]; then
    echo "deb.sh: $usage" >&2
    exit 2
fi
if ! [[ "$DEB_MAINTAINER" =~ ^[^\<\>@]+\ \<[^\<\>@\ ]+@[^\<\>@\ ]+\>$ ]]; then
    echo "deb.sh: DEB_MAINTAINER is not 'Name <address>'; $usage" >&2
    exit 2
fi
if ! command -v cargo-deb >/dev/null 2>&1; then
    echo "deb.sh: cargo-deb not found; install it with: cargo install cargo-deb --locked" >&2
    exit 2
fi

root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"

# The Debian changelog, from the template, with the maintainer and the date.
if [ -n "${SOURCE_DATE_EPOCH:-}" ]; then
    date=$(LC_ALL=C date -R -u -d "@$SOURCE_DATE_EPOCH")
else
    date=$(LC_ALL=C date -R)
fi
mkdir -p target/deb
changelog=target/deb/changelog
: >"$changelog"
while IFS= read -r line || [ -n "$line" ]; do
    line=${line//@MAINTAINER@/"$DEB_MAINTAINER"}
    line=${line//@DATE@/"$date"}
    printf '%s\n' "$line" >>"$changelog"
done <irida/res/debian/changelog.in

# The .deb ships two binaries from two packages, so the whole workspace is
# built first and cargo deb only packages.
cargo build --release --workspace --locked
out=$(cargo deb -p irida --locked --no-build --maintainer "$DEB_MAINTAINER")
deb=$(printf '%s\n' "$out" | grep '\.deb$' | tail -n 1)
if [ -z "$deb" ]; then
    echo "deb.sh: cargo deb did not report a .deb" >&2
    exit 1
fi
echo "$deb"
