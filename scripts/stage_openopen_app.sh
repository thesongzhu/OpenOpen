#!/bin/bash
set -euo pipefail

usage() {
  echo "usage: $0 --codex-package-root ABSOLUTE_PATH --imsg-binary ABSOLUTE_PATH --imsg-receipt ABSOLUTE_PATH --output ABSOLUTE_PATH" >&2
  exit 64
}

codex_root=""
imsg_binary=""
imsg_receipt=""
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
    --imsg-binary)
      [[ $# -ge 2 ]] || usage
      imsg_binary="$2"
      shift 2
      ;;
    --imsg-receipt)
      [[ $# -ge 2 ]] || usage
      imsg_receipt="$2"
      shift 2
      ;;
    *) usage ;;
  esac
done

[[ "$codex_root" = /* && "$imsg_binary" = /* && "$imsg_receipt" = /* && "$output" = /* ]] || usage
[[ "$output" == *.app && "$output" != "/" ]] || usage
[[ ! -e "$output" ]] || {
  echo "refusing to overwrite existing output: $output" >&2
  exit 65
}
[[ -f "$codex_root/bin/codex" ]] || {
  echo "missing pinned Codex executable" >&2
  exit 65
}
[[ -x "$imsg_binary" && -f "$imsg_receipt" ]] || {
  echo "missing pinned imsg executable or receipt" >&2
  exit 65
}
imsg_root="$(cd "$(dirname "$imsg_binary")/.." && pwd -P)"
[[ "$imsg_binary" == "$imsg_root/bin/imsg" ]] || {
  echo "imsg executable must be the exact bin/imsg member of its runtime tree" >&2
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
notice_manifest_sha="818495226dda3332f711fc6d6408eacf1776e08fcddfa06342ab3f5196417839"
verify_sha "$notice_manifest_sha" "$repo_root/third_party/notices/manifest.json"
notice_text_count=0
for notice in "$repo_root"/third_party/notices/texts/*.txt; do
  notice_hash="$(basename "$notice" .txt)"
  verify_sha "$notice_hash" "$notice"
  notice_text_count=$((notice_text_count + 1))
done
[[ "$notice_text_count" -eq 597 ]] || {
  echo "third-party notice text closure is incomplete" >&2
  exit 66
}
/usr/bin/jq -e . "$imsg_receipt" >/dev/null
receipt_value() {
  /usr/bin/jq -er --arg key "$1" '.[$key]' "$imsg_receipt"
}
[[ "$(receipt_value schemaVersion)" == "2" \
  && "$(receipt_value component)" == "openclaw/imsg" \
  && "$(receipt_value version)" == "0.13.0" \
  && "$(receipt_value sourceCommit)" == "fa2f82d7dbda4c802d91c1d41bb6c53564ed2fdc" \
  && "$(receipt_value packageResolvedSha256)" == "642390f861e9581bc0ec6e4b43abfb18bbbb20e37e7b130c35832a0e50b66054" \
  && "$(receipt_value surface)" == "openopen-basic-json-rpc-stdio" ]] || {
  echo "pinned imsg receipt metadata mismatch" >&2
  exit 66
}
imsg_patch_sha="$(/usr/bin/shasum -a 256 "$repo_root/third_party/imsg/openopen-basic-rpc.patch" | /usr/bin/awk '{print $1}')"
[[ "$(receipt_value patchSha256)" == "$imsg_patch_sha" ]] || {
  echo "pinned imsg patch receipt mismatch" >&2
  exit 66
}
verify_sha "$(/usr/bin/jq -er '.binary.sha256' "$imsg_receipt")" "$imsg_binary"
while IFS=$'\t' read -r relative expected_size expected_sha; do
  candidate="$imsg_root/$relative"
  [[ -f "$candidate" && ! -L "$candidate" \
    && "$(/usr/bin/stat -f '%z' "$candidate")" == "$expected_size" ]] || {
    echo "pinned imsg resource shape mismatch: $relative" >&2
    exit 66
  }
  verify_sha "$expected_sha" "$candidate"
done < <(/usr/bin/jq -r '.resources.files[] | [.path, (.size|tostring), .sha256] | @tsv' "$imsg_receipt")
[[ -z "$(/usr/bin/find -P "$imsg_root" -type l -print -quit)" \
  && -z "$(/usr/bin/find -P "$imsg_root" ! -type d ! -type f -print -quit)" ]] || {
  echo "pinned imsg runtime contains an alias or non-regular entry" >&2
  exit 66
}
imsg_receipt_sha="$(/usr/bin/shasum -a 256 "$imsg_receipt" | /usr/bin/awk '{print $1}')"

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
  "$app/Contents/Resources/iMessage/0.13.0/bin" \
  "$app/Contents/Resources/Notices" \
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
mkdir -p \
  "$app/Contents/Resources/Codex/0.144.0/bin" \
  "$app/Contents/Resources/Codex/0.144.0/codex-path"
/usr/bin/ditto "$codex_root/bin/codex" "$app/Contents/Resources/Codex/0.144.0/bin/codex"
/usr/bin/ditto "$codex_root/bin/codex-code-mode-host" \
  "$app/Contents/Resources/Codex/0.144.0/bin/codex-code-mode-host"
/usr/bin/ditto "$codex_root/codex-package.json" \
  "$app/Contents/Resources/Codex/0.144.0/codex-package.json"
/usr/bin/ditto "$codex_root/codex-path/rg" \
  "$app/Contents/Resources/Codex/0.144.0/codex-path/rg"
/usr/bin/ditto "$imsg_root" "$app/Contents/Resources/iMessage/0.13.0"
/usr/bin/ditto "$imsg_receipt" "$app/Contents/Resources/iMessage/0.13.0/BUILD-RECEIPT.json"
/usr/bin/ditto THIRD_PARTY_NOTICES.md "$app/Contents/Resources/Notices/THIRD_PARTY_NOTICES.md"
/usr/bin/ditto PROVENANCE.md "$app/Contents/Resources/Notices/PROVENANCE.md"
/usr/bin/ditto third_party/notices "$app/Contents/Resources/Notices/third_party"
/usr/bin/plutil -lint "$app/Contents/Info.plist"
/usr/bin/plutil -lint \
  "$app/Contents/Library/LaunchDaemons/com.thesongzhu.OpenOpen.EffectBroker.plist"
verify_sha "978740e6bcbd9af2f850823b723fb74f16d8d1e44de05f7dd6737ae631f72017" "$app/Contents/Resources/Codex/0.144.0/bin/codex"
verify_sha "c1b3af67fd28bbf768765357251f7b29d315150068cb41101dc77eb8a42bc7eb" "$app/Contents/Resources/Codex/0.144.0/codex-package.json"
verify_sha "067ee7894c49489ca72fc2ca6093f408302241bd22097fcd12d785c9ba40fd43" "$app/Contents/Resources/Codex/0.144.0/bin/codex-code-mode-host"
verify_sha "4fdf1d8365af224bc70e3c1490d8461d859c37cc70e739a11e987af0215f3e94" "$app/Contents/Resources/Codex/0.144.0/codex-path/rg"
actual_codex_files="$staging/codex-files.txt"
(cd "$app/Contents/Resources/Codex/0.144.0" && /usr/bin/find -P . -type f -print | /usr/bin/sed 's#^./##' | LC_ALL=C sort) >"$actual_codex_files"
/usr/bin/printf '%s\n' bin/codex bin/codex-code-mode-host codex-package.json codex-path/rg \
  | LC_ALL=C sort >"$staging/expected-codex-files.txt"
/usr/bin/cmp -s "$staging/expected-codex-files.txt" "$actual_codex_files" || {
  echo "staged Codex runtime contains an unexpected or missing file" >&2
  exit 66
}
verify_sha "$(/usr/bin/jq -er '.binary.sha256' "$imsg_receipt")" "$app/Contents/Resources/iMessage/0.13.0/bin/imsg"
verify_sha "$imsg_receipt_sha" "$app/Contents/Resources/iMessage/0.13.0/BUILD-RECEIPT.json"
/usr/bin/codesign --force --sign - --identifier com.thesongzhu.OpenOpen.imsg \
  "$app/Contents/Resources/iMessage/0.13.0/bin/imsg"
imsg_signed_sha="$(/usr/bin/shasum -a 256 "$app/Contents/Resources/iMessage/0.13.0/bin/imsg" | /usr/bin/awk '{print $1}')"
imsg_cdhash="$(/usr/bin/codesign -d --verbose=4 "$app/Contents/Resources/iMessage/0.13.0/bin/imsg" 2>&1 | /usr/bin/awk -F= '$1 == "CDHash" {print $2}')"
imsg_team="$(/usr/bin/codesign -d --verbose=4 "$app/Contents/Resources/iMessage/0.13.0/bin/imsg" 2>&1 | /usr/bin/awk -F= '$1 == "TeamIdentifier" {print $2}')"
resource_manifest="$staging/imsg-resource-manifest.txt"
: >"$resource_manifest"
while IFS= read -r relative; do
  file="$app/Contents/Resources/iMessage/0.13.0/$relative"
  /usr/bin/printf '%s\t%s\t%s\n' "$relative" "$(/usr/bin/stat -f '%z' "$file")" \
    "$(/usr/bin/shasum -a 256 "$file" | /usr/bin/awk '{print $1}')" >>"$resource_manifest"
done < <(/usr/bin/jq -r '.resources.files[].path' "$imsg_receipt" | LC_ALL=C sort)
resource_tree_sha="$(/usr/bin/shasum -a 256 "$resource_manifest" | /usr/bin/awk '{print $1}')"
[[ "$resource_tree_sha" == "$(/usr/bin/jq -er '.resources.treeSha256' "$imsg_receipt")" ]] || {
  echo "staged imsg resource tree drifted before signing" >&2
  exit 66
}
/usr/bin/jq -n \
  --arg build_receipt_sha256 "$imsg_receipt_sha" \
  --arg binary_sha256 "$imsg_signed_sha" \
  --arg resource_tree_sha256 "$resource_tree_sha" \
  --arg signing_identifier "com.thesongzhu.OpenOpen.imsg" \
  --arg team_identifier "$imsg_team" \
  --arg cdhash "$imsg_cdhash" \
  '{schemaVersion: 1, buildReceiptSha256: $build_receipt_sha256,
    binarySha256: $binary_sha256, resourceTreeSha256: $resource_tree_sha256,
    signingIdentifier: $signing_identifier, teamIdentifier: $team_identifier, cdhash: $cdhash}' \
  >"$app/Contents/Resources/iMessage/0.13.0/RUNTIME-RECEIPT.json"
staged_imsg_probe_db="$staging/staged-imsg-probe.sqlite"
/usr/bin/sqlite3 "$staged_imsg_probe_db" 'PRAGMA user_version = 1;'
/usr/bin/printf '%s\n' \
  '{"jsonrpc":"2.0","id":"stage-probe","method":"private.unavailable","params":{}}' \
  | "$app/Contents/Resources/iMessage/0.13.0/bin/imsg" rpc --db "$staged_imsg_probe_db" \
  | /usr/bin/jq -e '.id == "stage-probe" and .error.code == -32601' >/dev/null || {
    echo "signed staged imsg failed its adjacent-resource RPC probe" >&2
    exit 66
  }
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
verify_signing_field "$app/Contents/Resources/iMessage/0.13.0/bin/imsg" Identifier \
  com.thesongzhu.OpenOpen.imsg
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
verify_signing_field "$output/Contents/Resources/iMessage/0.13.0/bin/imsg" Identifier \
  com.thesongzhu.OpenOpen.imsg
verify_sha "$imsg_receipt_sha" "$output/Contents/Resources/iMessage/0.13.0/BUILD-RECEIPT.json"
verify_sha "$imsg_signed_sha" "$output/Contents/Resources/iMessage/0.13.0/bin/imsg"
/usr/bin/jq -e --arg build "$imsg_receipt_sha" --arg binary "$imsg_signed_sha" \
  '.schemaVersion == 1 and .buildReceiptSha256 == $build and .binarySha256 == $binary' \
  "$output/Contents/Resources/iMessage/0.13.0/RUNTIME-RECEIPT.json" >/dev/null
verify_sha "$notice_manifest_sha" \
  "$output/Contents/Resources/Notices/third_party/manifest.json"
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
