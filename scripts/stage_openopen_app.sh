#!/bin/bash
set -euo pipefail

usage() {
  echo "usage: $0 --codex-package-root ABSOLUTE_PATH --output ABSOLUTE_PATH" >&2
  exit 64
}

codex_root=""
output=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --codex-package-root)
      [[ $# -ge 2 ]] || usage
      codex_root="$2"
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

[[ "$codex_root" = /* && "$output" = /* ]] || usage
[[ "$output" == *.app && "$output" != "/" ]] || usage
[[ ! -e "$output" ]] || {
  echo "refusing to overwrite existing output: $output" >&2
  exit 65
}
[[ -f "$codex_root/bin/codex" ]] || {
  echo "missing pinned Codex executable" >&2
  exit 65
}

repo_root="$(cd "$(dirname "$0")/.." && pwd -P)"
codex_root="$(cd "$codex_root" && pwd -P)"
output_parent="$(dirname "$output")"
mkdir -p "$output_parent"
output_parent="$(cd "$output_parent" && pwd -P)"
output="$output_parent/$(basename "$output")"

verify_sha() {
  local expected="$1"
  local path="$2"
  local actual
  actual="$(/usr/bin/shasum -a 256 "$path" | /usr/bin/awk '{print $1}')"
  [[ "$actual" == "$expected" ]] || {
    echo "pinned component hash mismatch: $path" >&2
    exit 66
  }
}

verify_signing_field() {
  local path="$1"
  local field="$2"
  local expected="$3"
  local actual
  actual="$(/usr/bin/codesign -d --verbose=4 "$path" 2>&1 \
    | /usr/bin/awk -F= -v field="$field" '$1 == field {print $2}')"
  [[ "$actual" == "$expected" ]] || {
    echo "unexpected $field for $path: $actual" >&2
    exit 66
  }
}

verify_sha "978740e6bcbd9af2f850823b723fb74f16d8d1e44de05f7dd6737ae631f72017" "$codex_root/bin/codex"
verify_sha "c1b3af67fd28bbf768765357251f7b29d315150068cb41101dc77eb8a42bc7eb" "$codex_root/codex-package.json"
verify_sha "067ee7894c49489ca72fc2ca6093f408302241bd22097fcd12d785c9ba40fd43" "$codex_root/bin/codex-code-mode-host"
verify_sha "4fdf1d8365af224bc70e3c1490d8461d859c37cc70e739a11e987af0215f3e94" "$codex_root/codex-path/rg"

cd "$repo_root"
cargo build --release -p openopen-host -p openopen-effect-broker
swift build \
  --package-path macos/EffectBrokerBridge \
  --product OpenOpen \
  --configuration release \
  -Xswiftc -warnings-as-errors
swift build \
  --package-path macos/EffectBrokerBridge \
  --product OpenOpenEffectBroker \
  --configuration release \
  -Xswiftc -warnings-as-errors
swift_bin="$(swift build --package-path macos/EffectBrokerBridge --show-bin-path --configuration release)"

staging="$(/usr/bin/mktemp -d "$output_parent/.OpenOpen-stage.XXXXXX")"
claimed_output=0
cleanup() {
  rm -rf "$staging"
  if [[ "$claimed_output" -eq 1 ]]; then
    rm -rf "$output"
  fi
}
trap cleanup EXIT
app="$staging/OpenOpen.app"
mkdir -p \
  "$app/Contents/MacOS" \
  "$app/Contents/Resources/Codex/0.144.0" \
  "$app/Contents/Library/LaunchDaemons"
/usr/bin/ditto macos/OpenOpenApp/Info.plist "$app/Contents/Info.plist"
/usr/bin/ditto "$swift_bin/OpenOpen" "$app/Contents/MacOS/OpenOpen"
/usr/bin/ditto target/release/openopen-host "$app/Contents/MacOS/OpenOpenCore"
/usr/bin/ditto "$swift_bin/OpenOpenEffectBroker" "$app/Contents/MacOS/OpenOpenEffectBroker"
/usr/bin/ditto \
  target/release/OpenOpenEffectBrokerWorker \
  "$app/Contents/MacOS/OpenOpenEffectBrokerWorker"
/usr/bin/ditto \
  macos/EffectBrokerBridge/Sources/EffectBrokerBridge/Resources/LaunchDaemons/com.thesongzhu.OpenOpen.EffectBroker.plist \
  "$app/Contents/Library/LaunchDaemons/com.thesongzhu.OpenOpen.EffectBroker.plist"
/usr/bin/ditto "$codex_root" "$app/Contents/Resources/Codex/0.144.0"
/usr/bin/plutil -lint "$app/Contents/Info.plist"
/usr/bin/plutil -lint \
  "$app/Contents/Library/LaunchDaemons/com.thesongzhu.OpenOpen.EffectBroker.plist"
verify_sha "978740e6bcbd9af2f850823b723fb74f16d8d1e44de05f7dd6737ae631f72017" "$app/Contents/Resources/Codex/0.144.0/bin/codex"
verify_sha "c1b3af67fd28bbf768765357251f7b29d315150068cb41101dc77eb8a42bc7eb" "$app/Contents/Resources/Codex/0.144.0/codex-package.json"
verify_sha "067ee7894c49489ca72fc2ca6093f408302241bd22097fcd12d785c9ba40fd43" "$app/Contents/Resources/Codex/0.144.0/bin/codex-code-mode-host"
verify_sha "4fdf1d8365af224bc70e3c1490d8461d859c37cc70e739a11e987af0215f3e94" "$app/Contents/Resources/Codex/0.144.0/codex-path/rg"
/usr/bin/codesign --force --sign - --identifier com.thesongzhu.OpenOpen \
  "$app/Contents/MacOS/OpenOpen"
/usr/bin/codesign --force --sign - --identifier com.thesongzhu.OpenOpen.Core \
  "$app/Contents/MacOS/OpenOpenCore"
/usr/bin/codesign --force --sign - --identifier com.thesongzhu.OpenOpen.EffectBroker \
  "$app/Contents/MacOS/OpenOpenEffectBroker"
/usr/bin/codesign --force --sign - \
  --identifier com.thesongzhu.OpenOpen.EffectBroker.Worker \
  "$app/Contents/MacOS/OpenOpenEffectBrokerWorker"
/usr/bin/codesign --force --sign - --identifier com.thesongzhu.OpenOpen "$app"
/usr/bin/codesign --verify --deep --strict "$app"
verify_signing_field "$app/Contents/MacOS/OpenOpenCore" Identifier \
  com.thesongzhu.OpenOpen.Core
verify_signing_field "$app/Contents/MacOS/OpenOpenEffectBroker" Identifier \
  com.thesongzhu.OpenOpen.EffectBroker
verify_signing_field "$app/Contents/MacOS/OpenOpenEffectBrokerWorker" Identifier \
  com.thesongzhu.OpenOpen.EffectBroker.Worker
verify_signing_field "$app/Contents/Resources/Codex/0.144.0/bin/codex" Identifier codex
verify_signing_field "$app/Contents/Resources/Codex/0.144.0/bin/codex" TeamIdentifier \
  2DC432GLL2
verify_signing_field "$app/Contents/Resources/Codex/0.144.0/bin/codex" CDHash \
  cf4f00c153b0ef5af3f71281d1a6c47be9c85c8e

mkdir "$output" || {
  echo "refusing to overwrite existing output: $output" >&2
  exit 65
}
claimed_output=1
/usr/bin/ditto "$app/Contents" "$output/Contents"
/usr/bin/codesign --verify --deep --strict "$output"
verify_signing_field "$output/Contents/MacOS/OpenOpenCore" Identifier \
  com.thesongzhu.OpenOpen.Core
verify_signing_field "$output/Contents/MacOS/OpenOpenEffectBroker" Identifier \
  com.thesongzhu.OpenOpen.EffectBroker
verify_signing_field "$output/Contents/MacOS/OpenOpenEffectBrokerWorker" Identifier \
  com.thesongzhu.OpenOpen.EffectBroker.Worker
verify_signing_field "$output/Contents/Resources/Codex/0.144.0/bin/codex" Identifier codex
verify_signing_field "$output/Contents/Resources/Codex/0.144.0/bin/codex" TeamIdentifier \
  2DC432GLL2
verify_signing_field "$output/Contents/Resources/Codex/0.144.0/bin/codex" CDHash \
  cf4f00c153b0ef5af3f71281d1a6c47be9c85c8e
verify_sha "978740e6bcbd9af2f850823b723fb74f16d8d1e44de05f7dd6737ae631f72017" "$output/Contents/Resources/Codex/0.144.0/bin/codex"
verify_sha "c1b3af67fd28bbf768765357251f7b29d315150068cb41101dc77eb8a42bc7eb" "$output/Contents/Resources/Codex/0.144.0/codex-package.json"
verify_sha "067ee7894c49489ca72fc2ca6093f408302241bd22097fcd12d785c9ba40fd43" "$output/Contents/Resources/Codex/0.144.0/bin/codex-code-mode-host"
verify_sha "4fdf1d8365af224bc70e3c1490d8461d859c37cc70e739a11e987af0215f3e94" "$output/Contents/Resources/Codex/0.144.0/codex-path/rg"
claimed_output=0
echo "STAGED_AD_HOC_NOT_RELEASE_PROOF $output"
