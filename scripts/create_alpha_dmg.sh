#!/bin/bash
set -euo pipefail
umask 077

usage() {
  echo "usage: $0 --app ABSOLUTE_PATH --output ABSOLUTE_PATH" >&2
  exit 64
}

app=""
output=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --app)
      [[ $# -ge 2 ]] || usage
      app="$2"
      shift 2
      ;;
    --output)
      [[ $# -ge 2 ]] || usage
      output="$2"
      shift 2
      ;;
    *) usage ;;
  esac
done

[[ "$app" = /* && "$output" = /* && "$output" == *.dmg ]] || usage
[[ -d "$app/Contents" ]] || {
  echo "missing staged OpenOpen app" >&2
  exit 65
}
[[ ! -e "$output" ]] || {
  echo "refusing to overwrite existing DMG: $output" >&2
  exit 65
}
/usr/bin/codesign --verify --deep --strict "$app"

output_parent="$(dirname "$output")"
mkdir -p "$output_parent"
output_parent="$(cd "$output_parent" && pwd -P)"
output="$output_parent/$(basename "$output")"
staging="$(/usr/bin/mktemp -d "$output_parent/.OpenOpen-alpha-dmg.XXXXXX")"
mountpoint="$(/usr/bin/mktemp -d /private/tmp/OpenOpen-alpha-mount.XXXXXX)"
mounted=0
claimed_output=0
cleanup() {
  if [[ "$mounted" -eq 1 ]]; then
    /usr/bin/hdiutil detach "$mountpoint" -quiet || true
  fi
  rm -rf "$mountpoint" "$staging"
  if [[ "$claimed_output" -eq 1 ]]; then
    rm -f "$output"
  fi
}
trap cleanup EXIT

disk_root="$staging/disk"
install_root="$staging/install-test"
mkdir "$disk_root" "$install_root"
/usr/bin/ditto "$app" "$disk_root/OpenOpen.app"
ln -s /Applications "$disk_root/Applications"
/usr/bin/hdiutil create \
  -quiet \
  -volname "OpenOpen Alpha" \
  -srcfolder "$disk_root" \
  -format UDZO \
  "$output"
claimed_output=1

/usr/bin/hdiutil attach \
  -quiet \
  -readonly \
  -nobrowse \
  -mountpoint "$mountpoint" \
  "$output"
mounted=1
[[ -d "$mountpoint/OpenOpen.app/Contents" ]] || {
  echo "mounted alpha DMG is missing OpenOpen.app" >&2
  exit 66
}
/usr/bin/codesign --verify --deep --strict "$mountpoint/OpenOpen.app"
/usr/bin/ditto "$mountpoint/OpenOpen.app" "$install_root/OpenOpen.app"
/usr/bin/codesign --verify --deep --strict "$install_root/OpenOpen.app"
/usr/bin/hdiutil detach "$mountpoint" -quiet
mounted=0

dmg_sha="$(/usr/bin/shasum -a 256 "$output" | /usr/bin/awk '{print $1}')"
claimed_output=0
echo "ALPHA_DMG_AD_HOC_NOT_NOTARIZED_NOT_RELEASE_PROOF $dmg_sha $output"
