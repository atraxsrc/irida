#!/usr/bin/env bash
# Check a built Irida .deb: its control data, the files it installs and
# their modes, the desktop entry, the AppStream data and, when installed,
# lintian. Exits non-zero on the first failed check.
set -euo pipefail

if [ "$#" -ne 1 ] || [ ! -f "$1" ]; then
    echo "usage: scripts/check-deb.sh <path to irida .deb>" >&2
    exit 2
fi
deb=$1
app_id=io.github.atraxsrc.Irida

fail() {
    echo "check-deb: $*" >&2
    exit 1
}

echo "== dpkg-deb --info"
dpkg-deb --info "$deb"

echo "== dpkg-deb --contents"
contents=$(dpkg-deb --contents "$deb")
echo "$contents"

# Each installed file with the mode it must have.
expected=(
    "-rwxr-xr-x ./usr/bin/irida"
    "-rw-r--r-- ./usr/share/applications/$app_id.desktop"
    "-rw-r--r-- ./usr/share/metainfo/$app_id.metainfo.xml"
    "-rw-r--r-- ./usr/share/icons/hicolor/scalable/apps/$app_id.svg"
    "-rw-r--r-- ./usr/share/doc/irida/copyright"
    "-rw-r--r-- ./usr/share/doc/irida/changelog.Debian.gz"
)
for entry in "${expected[@]}"; do
    mode=${entry%% *}
    path=${entry#* }
    line=$(awk -v p="$path" '$NF == p' <<<"$contents")
    [ -n "$line" ] || fail "missing $path"
    [ "${line%% *}" = "$mode" ] || fail "$path has mode ${line%% *}, expected $mode"
    echo "ok $mode $path"
done

tmp=$(mktemp -d "${TMPDIR:-/tmp}/irida-check-deb.XXXXXX")
trap 'rm -rf "$tmp"' EXIT
dpkg-deb -x "$deb" "$tmp"

echo "== desktop-file-validate"
desktop-file-validate "$tmp/usr/share/applications/$app_id.desktop"
echo "ok desktop entry"

echo "== appstreamcli validate"
appstreamcli validate --no-net "$tmp/usr/share/metainfo/$app_id.metainfo.xml"
appstreamcli validate-tree --no-net "$tmp"
echo "ok AppStream data"

if command -v lintian >/dev/null 2>&1; then
    echo "== lintian"
    # Errors and warnings fail, except the two every first upload of a Rust
    # binary gets. Info tags are shown but come from the toolchain (fortify,
    # md5sums, strings inside dependencies), so they do not fail.
    lintian --display-info --fail-on error,warning \
        --suppress-tags initial-upload-closes-no-bugs,no-manual-page "$deb"
    echo "ok lintian"
else
    echo "== lintian not installed, skipped"
fi

echo "check-deb: all checks passed"
