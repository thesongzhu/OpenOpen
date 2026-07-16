#!/bin/bash
set -euo pipefail

usage() {
  echo "usage: $0 --codex-package-root ABSOLUTE_PATH --imsg-binary ABSOLUTE_PATH --imsg-receipt ABSOLUTE_PATH --output ABSOLUTE_PATH [--developer-id-identity CERTIFICATE_NAME]" >&2
  exit 64
}

codex_root=""
imsg_binary=""
imsg_receipt=""
output=""
signing_identity="-"
signing_mode="ad-hoc"
readonly expected_developer_id_identity="Developer ID Application: Wenxin Dou (UHDY2275L5)"
readonly expected_developer_id_leaf_sha="a7e43925d8ee4ad927f6ac27078eff554b7487a58f73b8f3acd7fabadc4057c8"
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
    --developer-id-identity)
      [[ $# -ge 2 && -n "$2" ]] || usage
      signing_identity="$2"
      signing_mode="developer-id"
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
imsg_entitlements="$repo_root/macos/OpenOpenApp/iMessage.entitlements"
[[ -f "$imsg_entitlements" ]] || {
  echo "missing iMessage automation entitlements" >&2
  exit 65
}
if [[ "$signing_mode" == "developer-id" ]]; then
  [[ "$signing_identity" == "$expected_developer_id_identity" ]] || {
    echo "Developer-ID mode requires the pinned owner Developer ID Application certificate" >&2
    exit 65
  }
  /usr/bin/security find-identity -v -p codesigning \
    | /usr/bin/grep -F -- "\"$signing_identity\"" >/dev/null || {
      echo "requested Developer ID Application identity is unavailable" >&2
      exit 65
    }
fi
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

certificate_leaf_sha() {
  local path="$1"
  local scratch leaf_sha
  scratch="$(/usr/bin/mktemp -d /private/tmp/OpenOpen-certificate.XXXXXX)"
  /usr/bin/codesign -d --extract-certificates="$scratch/certificate" "$path" \
    >/dev/null 2>&1 || {
    rm -rf "$scratch"
    return 1
  }
  [[ -f "$scratch/certificate0" ]] || {
    rm -rf "$scratch"
    return 1
  }
  leaf_sha="$(/usr/bin/shasum -a 256 "$scratch/certificate0" | /usr/bin/awk '{print $1}')"
  rm -rf "$scratch"
  /usr/bin/printf '%s\n' "$leaf_sha"
}

verify_owner_certificate() {
  local path="$1"
  local actual
  actual="$(certificate_leaf_sha "$path")" || {
    echo "unable to extract Developer ID leaf certificate: $path" >&2
    exit 66
  }
  [[ "$actual" == "$expected_developer_id_leaf_sha" ]] || {
    echo "unexpected Developer ID leaf certificate: $path" >&2
    exit 66
  }
}

verify_unsigned_macho_sha() {
  local expected="$1"
  local path="$2"
  local scratch actual
  scratch="$(/usr/bin/mktemp /private/tmp/OpenOpen-unsigned-macho.XXXXXX)"
  /usr/bin/ditto "$path" "$scratch" || {
    rm -f "$scratch"
    exit 66
  }
  /usr/bin/codesign --remove-signature "$scratch" >/dev/null 2>&1 || {
    rm -f "$scratch"
    exit 66
  }
  actual="$(/usr/bin/shasum -a 256 "$scratch" | /usr/bin/awk '{print $1}')"
  rm -f "$scratch"
  [[ "$actual" == "$expected" ]] || {
    echo "unsigned Mach-O content mismatch: $path" >&2
    exit 66
  }
}

normalize_app_modes() {
  local candidate="$1"
  /bin/chmod -R u+rwX "$candidate"
  /bin/chmod -RN "$candidate"
  /usr/bin/chflags -R nouchg,noschg "$candidate"
  /usr/bin/xattr -cr "$candidate"
  /usr/bin/find -P "$candidate" -type d -exec /bin/chmod 0755 {} +
  /usr/bin/find -P "$candidate" -type f -exec /bin/chmod 0644 {} +
  /bin/chmod 0755 \
    "$candidate/Contents/MacOS/OpenOpen" \
    "$candidate/Contents/MacOS/OpenOpenCore" \
    "$candidate/Contents/MacOS/OpenOpenEffectBroker" \
    "$candidate/Contents/MacOS/OpenOpenEffectBrokerWorker" \
    "$candidate/Contents/Resources/Codex/0.144.0/bin/codex" \
    "$candidate/Contents/Resources/Codex/0.144.0/bin/codex-code-mode-host" \
    "$candidate/Contents/Resources/Codex/0.144.0/codex-path/rg" \
    "$candidate/Contents/Resources/iMessage/0.13.0/bin/imsg"
}

verify_app_operational_metadata() {
  local candidate="$1"
  local path flags acl_lines attribute
  while IFS= read -r path; do
    flags="$(/usr/bin/stat -f '%f' "$path")"
    [[ "$flags" == "0" ]] || {
      echo "staged app entry has forbidden BSD flags: $path" >&2
      exit 66
    }
    acl_lines="$(/bin/ls -lde "$path" | /usr/bin/wc -l | /usr/bin/tr -d ' ')"
    [[ "$acl_lines" == "1" ]] || {
      echo "staged app entry has a forbidden ACL: $path" >&2
      exit 66
    }
    while IFS= read -r attribute; do
      [[ -z "$attribute" || "$attribute" == "com.apple.provenance" ]] || {
        echo "staged app entry has a forbidden extended attribute: $path" >&2
        exit 66
      }
    done < <(/usr/bin/xattr "$path" 2>/dev/null || true)
  done < <(/usr/bin/find -P "$candidate" -print)
}

verify_app_modes() {
  local candidate="$1"
  local path mode expected
  while IFS= read -r path; do
    mode="$(/usr/bin/stat -f '%Lp' "$path")"
    [[ "$mode" == "755" ]] || {
      echo "staged app directory mode mismatch: $path" >&2
      exit 66
    }
  done < <(/usr/bin/find -P "$candidate" -type d -print)
  while IFS= read -r path; do
    mode="$(/usr/bin/stat -f '%Lp' "$path")"
    expected="644"
    if [[ "$(/usr/bin/file -b "$path")" == *"Mach-O"* ]]; then
      expected="755"
    fi
    [[ "$mode" == "$expected" ]] || {
      echo "staged app file mode mismatch: $path" >&2
      exit 66
    }
  done < <(/usr/bin/find -P "$candidate" -type f -print)
}

sign_owned_code() {
  local path="$1"
  local identifier="$2"
  local entitlements="${3:-}"
  local unsigned_sha_before="${4:-}"
  local unsigned_sha_after="${5:-${4:-}}"
  local arguments=(--force --sign "$signing_identity" --identifier "$identifier")
  if [[ -n "$unsigned_sha_before" ]]; then
    verify_unsigned_macho_sha "$unsigned_sha_before" "$path"
  fi
  if [[ "$signing_mode" == "developer-id" ]]; then
    arguments+=(--options runtime --timestamp)
  fi
  if [[ -n "$entitlements" ]]; then
    arguments+=(--entitlements "$entitlements")
  fi
  /usr/bin/codesign "${arguments[@]}" "$path"
  if [[ -n "$unsigned_sha_after" ]]; then
    verify_unsigned_macho_sha "$unsigned_sha_after" "$path"
  fi
  if [[ "$signing_mode" == "developer-id" ]]; then
    verify_owner_certificate "$path"
  fi
}

verify_hardened_timestamped() {
  local path="$1"
  local details
  details="$(/usr/bin/codesign -d --verbose=4 "$path" 2>&1)"
  /usr/bin/grep -E 'flags=.*\(runtime\)' <<<"$details" >/dev/null || {
    echo "hardened runtime is missing for $path" >&2
    exit 66
  }
  /usr/bin/grep -E '^Timestamp=' <<<"$details" >/dev/null || {
    echo "secure timestamp is missing for $path" >&2
    exit 66
  }
}

verify_developer_id_application() {
  local path="$1"
  local team_identifier="$2"
  local requirement
  requirement="=anchor apple generic and certificate leaf[field.1.2.840.113635.100.6.1.13] /* exists */ and certificate leaf[subject.OU] = \"$team_identifier\""
  /usr/bin/codesign --verify --strict --test-requirement "$requirement" "$path" || {
    echo "signature is not an Apple Developer ID Application for Team $team_identifier: $path" >&2
    exit 66
  }
}

verify_no_get_task_allow() {
  local path="$1"
  local value
  value="$({ /usr/bin/codesign -d --entitlements :- "$path" 2>/dev/null || true; } \
    | /usr/bin/plutil -extract 'com\.apple\.security\.get-task-allow' raw - \
      2>/dev/null || true)"
  [[ "$value" != "true" ]] || {
    echo "get-task-allow is forbidden in a distribution signature: $path" >&2
    exit 66
  }
}

verify_sha "978740e6bcbd9af2f850823b723fb74f16d8d1e44de05f7dd6737ae631f72017" "$codex_root/bin/codex"
verify_sha "c1b3af67fd28bbf768765357251f7b29d315150068cb41101dc77eb8a42bc7eb" "$codex_root/codex-package.json"
verify_sha "067ee7894c49489ca72fc2ca6093f408302241bd22097fcd12d785c9ba40fd43" "$codex_root/bin/codex-code-mode-host"
codex_rg_upstream_sha="4fdf1d8365af224bc70e3c1490d8461d859c37cc70e739a11e987af0215f3e94"
verify_sha "$codex_rg_upstream_sha" "$codex_root/codex-path/rg"
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
readonly expected_imsg_receipt_sha="c1769b4093faa6e8bde56cdb16ad2c950ee39ea5501630e0ba022901b56a7b3d"
readonly expected_imsg_binary_sha="635c99814fc3dbefffacaeb5222d4bf2ed340d019e726751ead909addc9122a1"
readonly expected_imsg_binary_size="2703768"
readonly expected_imsg_patch_sha="94b4b09ad605d8d6e1ff9c112897f730578cb3c4c8a1571340b92e1b0710717c"
readonly expected_imsg_runtime_tree_sha="df28709875c9cbae922f13a5974368c7a02b5d750a4552ed79ff7aa6c9180704"
readonly expected_imsg_resource_tree_sha="7a5cb869823a893a7181bcacfef6dfc8be335a5ce2bf14caac579096f78909cc"
readonly expected_imsg_core_sources_sha="3da18561485472996b3d74d820d87fc936580f9b05d3c5c2c3ab3e45f0323d27"
readonly expected_imsg_cli_sources_sha="2f39e12fcf0879359c3f16a60061d7ab9fadcf14e4bd7e2109b76e49b764a7c6"
readonly expected_imsg_unsigned_sha="cdea42cf30e731d52c00524c16db5865fe2d01ef6a3f377cb6e3a3eb65f5f313"
readonly expected_rg_unsigned_before_sha="7894fcced308b247aee2315d133e0670d73e608bfb41d8bb003665cc31328c47"
readonly expected_rg_unsigned_sha="ea91b02e833a93bea206911bb80434a837d11a4d2eca520548abd07cece2c2c6"
readonly expected_app_unsigned_sha="62b7b0aaa2a222d4679bcb6a759ef58061c64afe7be22577c7ffbf8ecf503d98"
readonly expected_core_unsigned_before_sha="25437c31712fbecf0c9d94d6cb1dae8f5ffa34cff34519b91d068fb1a1492b3a"
readonly expected_core_unsigned_sha="47b4fae44d7fb2bbe089beb6db17003b4e96691d2fd120d10eaafe6a52c0c60a"
readonly expected_broker_unsigned_sha="a3a4f173957891464f3e4e8c6a9d878514811669527a2a763e1c72d28ad89236"
readonly expected_worker_unsigned_sha="af9c72d3eba3adab68ddb6f6d89997f92900fb7826f3be80ad510a2007fd7d05"
verify_sha "$expected_imsg_receipt_sha" "$imsg_receipt"
imsg_receipt_sha="$expected_imsg_receipt_sha"
receipt_value() {
  /usr/bin/jq -er --arg key "$1" '.[$key]' "$imsg_receipt"
}
/usr/bin/jq -e \
  --arg receipt_sha "$expected_imsg_receipt_sha" \
  --arg binary_sha "$expected_imsg_binary_sha" \
  --argjson binary_size "$expected_imsg_binary_size" \
  --arg patch_sha "$expected_imsg_patch_sha" \
  --arg runtime_tree "$expected_imsg_runtime_tree_sha" \
  --arg resource_tree "$expected_imsg_resource_tree_sha" \
  --arg core_sources "$expected_imsg_core_sources_sha" \
  --arg cli_sources "$expected_imsg_cli_sources_sha" \
  'keys == ["binary", "compiledSources", "component", "packageResolvedSha256",
            "patchSha256", "resources", "runtimeTreeSha256", "schemaVersion",
            "sourceCommit", "surface", "version"]
   and .schemaVersion == 2
   and .component == "openclaw/imsg"
   and .version == "0.13.0"
   and .sourceCommit == "fa2f82d7dbda4c802d91c1d41bb6c53564ed2fdc"
   and .packageResolvedSha256 == "642390f861e9581bc0ec6e4b43abfb18bbbb20e37e7b130c35832a0e50b66054"
   and .patchSha256 == $patch_sha
   and .surface == "openopen-basic-json-rpc-stdio"
   and .runtimeTreeSha256 == $runtime_tree
   and .binary == {path: "bin/imsg", size: $binary_size, sha256: $binary_sha}
   and .resources.bundlePath == "bin/PhoneNumberKit_PhoneNumberKit.bundle"
   and .resources.treeSha256 == $resource_tree
   and .resources.files == [
     {path: "bin/PhoneNumberKit_PhoneNumberKit.bundle/PhoneNumberMetadata.json",
      size: 365606,
      sha256: "8ca856c3507586427462bfb2dfcfd3dfb94070e40efc272430b88481e71ba283"},
     {path: "bin/PhoneNumberKit_PhoneNumberKit.bundle/PrivacyInfo.xcprivacy",
      size: 372,
      sha256: "561040f7a52952f75d02d4b6758382ff48c563d6f25c84a09f1a655f6dc60ff8"}
   ]
   and .compiledSources.IMsgCore.manifestSha256 == $core_sources
   and (.compiledSources.IMsgCore.files | length) == 25
   and .compiledSources.imsg.manifestSha256 == $cli_sources
   and (.compiledSources.imsg.files | length) == 13' \
  "$imsg_receipt" >/dev/null || {
  echo "pinned imsg receipt metadata mismatch" >&2
  exit 66
}
imsg_patch_sha="$(/usr/bin/shasum -a 256 "$repo_root/third_party/imsg/openopen-basic-rpc.patch" | /usr/bin/awk '{print $1}')"
[[ "$imsg_patch_sha" == "$expected_imsg_patch_sha" \
  && "$(receipt_value patchSha256)" == "$imsg_patch_sha" ]] || {
  echo "pinned imsg patch receipt mismatch" >&2
  exit 66
}
[[ "$(/usr/bin/stat -f '%z' "$imsg_binary")" == "$expected_imsg_binary_size" ]] || {
  echo "pinned imsg binary size mismatch" >&2
  exit 66
}
verify_sha "$expected_imsg_binary_sha" "$imsg_binary"
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
actual_imsg_runtime_files="$(
  cd "$imsg_root"
  /usr/bin/find -P . -type f -print | /usr/bin/sed 's#^./##' | LC_ALL=C sort
)"
expected_imsg_runtime_files="$(/usr/bin/printf '%s\n' \
  bin/PhoneNumberKit_PhoneNumberKit.bundle/PhoneNumberMetadata.json \
  bin/PhoneNumberKit_PhoneNumberKit.bundle/PrivacyInfo.xcprivacy \
  bin/imsg | LC_ALL=C sort)"
[[ "$actual_imsg_runtime_files" == "$expected_imsg_runtime_files" ]] || {
  echo "pinned imsg runtime file allowlist mismatch" >&2
  exit 66
}
while IFS= read -r relative; do
  file_kind="$(/usr/bin/file -b "$imsg_root/$relative")"
  if [[ "$file_kind" == *"Mach-O"* && "$relative" != "bin/imsg" ]]; then
    echo "unexpected Mach-O in pinned imsg runtime: $relative" >&2
    exit 66
  fi
done <<<"$actual_imsg_runtime_files"
[[ "$(/usr/bin/file -b "$imsg_binary")" == *"Mach-O"* ]] || {
  echo "pinned imsg binary is not Mach-O" >&2
  exit 66
}
imsg_resource_manifest=""
while IFS= read -r relative; do
  case "$relative" in
    bin/PhoneNumberKit_PhoneNumberKit.bundle/*)
      imsg_resource_manifest+="${relative}"$'\t'"$(/usr/bin/stat -f '%z' "$imsg_root/$relative")"$'\t'"$(/usr/bin/shasum -a 256 "$imsg_root/$relative" | /usr/bin/awk '{print $1}')"$'\n'
      ;;
  esac
done <<<"$actual_imsg_runtime_files"
[[ "$(/usr/bin/printf '%s' "$imsg_resource_manifest" | /usr/bin/shasum -a 256 | /usr/bin/awk '{print $1}')" \
  == "$expected_imsg_resource_tree_sha" ]] || {
  echo "pinned imsg resource tree mismatch" >&2
  exit 66
}
imsg_runtime_manifest=""
while IFS= read -r relative; do
  imsg_runtime_manifest+="${relative}"$'\t'"$(/usr/bin/stat -f '%z' "$imsg_root/$relative")"$'\t'"$(/usr/bin/shasum -a 256 "$imsg_root/$relative" | /usr/bin/awk '{print $1}')"$'\n'
done <<<"$actual_imsg_runtime_files"
[[ "$(/usr/bin/printf '%s' "$imsg_runtime_manifest" | /usr/bin/shasum -a 256 | /usr/bin/awk '{print $1}')" \
  == "$expected_imsg_runtime_tree_sha" ]] || {
  echo "pinned imsg runtime tree mismatch" >&2
  exit 66
}
for compiled_section in IMsgCore imsg; do
  compiled_manifest="$(/usr/bin/jq -r ".compiledSources.${compiled_section}.files[]" "$imsg_receipt")"
  compiled_sha="$(/usr/bin/printf '%s\n' "$compiled_manifest" | /usr/bin/shasum -a 256 | /usr/bin/awk '{print $1}')"
  [[ "$compiled_sha" == "$(/usr/bin/jq -er ".compiledSources.${compiled_section}.manifestSha256" "$imsg_receipt")" ]] || {
    echo "pinned imsg compiled source manifest mismatch: $compiled_section" >&2
    exit 66
  }
done

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
normalize_app_modes "$app"
verify_app_modes "$app"
verify_app_operational_metadata "$app"
verify_sha "978740e6bcbd9af2f850823b723fb74f16d8d1e44de05f7dd6737ae631f72017" "$app/Contents/Resources/Codex/0.144.0/bin/codex"
verify_sha "c1b3af67fd28bbf768765357251f7b29d315150068cb41101dc77eb8a42bc7eb" "$app/Contents/Resources/Codex/0.144.0/codex-package.json"
verify_sha "067ee7894c49489ca72fc2ca6093f408302241bd22097fcd12d785c9ba40fd43" "$app/Contents/Resources/Codex/0.144.0/bin/codex-code-mode-host"
verify_sha "$codex_rg_upstream_sha" "$app/Contents/Resources/Codex/0.144.0/codex-path/rg"
actual_codex_files="$staging/codex-files.txt"
(cd "$app/Contents/Resources/Codex/0.144.0" && /usr/bin/find -P . -type f -print | /usr/bin/sed 's#^./##' | LC_ALL=C sort) >"$actual_codex_files"
/usr/bin/printf '%s\n' bin/codex bin/codex-code-mode-host codex-package.json codex-path/rg \
  | LC_ALL=C sort >"$staging/expected-codex-files.txt"
/usr/bin/cmp -s "$staging/expected-codex-files.txt" "$actual_codex_files" || {
  echo "staged Codex runtime contains an unexpected or missing file" >&2
  exit 66
}
codex_rg_runtime_sha="$codex_rg_upstream_sha"
codex_runtime_receipt=""
if [[ "$signing_mode" == "developer-id" ]]; then
  sign_owned_code \
    "$app/Contents/Resources/Codex/0.144.0/codex-path/rg" rg "" \
    "$expected_rg_unsigned_before_sha" \
    "$expected_rg_unsigned_sha"
  codex_rg_runtime_sha="$(/usr/bin/shasum -a 256 \
    "$app/Contents/Resources/Codex/0.144.0/codex-path/rg" | /usr/bin/awk '{print $1}')"
  codex_rg_cdhash="$(/usr/bin/codesign -d --verbose=4 \
    "$app/Contents/Resources/Codex/0.144.0/codex-path/rg" 2>&1 \
    | /usr/bin/awk -F= '$1 == "CDHash" {print $2}')"
  codex_rg_team="$(/usr/bin/codesign -d --verbose=4 \
    "$app/Contents/Resources/Codex/0.144.0/codex-path/rg" 2>&1 \
    | /usr/bin/awk -F= '$1 == "TeamIdentifier" {print $2}')"
  codex_runtime_receipt="$app/Contents/Resources/Notices/CODEX-RUNTIME-RECEIPT.json"
  /usr/bin/jq -n \
    --arg upstream_rg_sha256 "$codex_rg_upstream_sha" \
    --arg runtime_rg_sha256 "$codex_rg_runtime_sha" \
    --arg signing_identifier "rg" \
    --arg team_identifier "$codex_rg_team" \
    --arg cdhash "$codex_rg_cdhash" \
    '{schemaVersion: 1, component: "openai/codex", version: "0.144.0",
      upstreamRgSha256: $upstream_rg_sha256, runtimeRgSha256: $runtime_rg_sha256,
      signingIdentifier: $signing_identifier, teamIdentifier: $team_identifier,
      cdhash: $cdhash}' >"$codex_runtime_receipt"
fi
verify_sha "$(/usr/bin/jq -er '.binary.sha256' "$imsg_receipt")" "$app/Contents/Resources/iMessage/0.13.0/bin/imsg"
verify_sha "$imsg_receipt_sha" "$app/Contents/Resources/iMessage/0.13.0/BUILD-RECEIPT.json"
sign_owned_code \
  "$app/Contents/Resources/iMessage/0.13.0/bin/imsg" \
  com.thesongzhu.OpenOpen.imsg \
  "$imsg_entitlements" \
  "$expected_imsg_unsigned_sha"
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
sign_owned_code \
  "$app/Contents/MacOS/OpenOpen" com.thesongzhu.OpenOpen "" \
  "$expected_app_unsigned_sha"
sign_owned_code \
  "$app/Contents/MacOS/OpenOpenCore" com.thesongzhu.OpenOpen.Core "" \
  "$expected_core_unsigned_before_sha" \
  "$expected_core_unsigned_sha"
sign_owned_code \
  "$app/Contents/MacOS/OpenOpenEffectBroker" \
  com.thesongzhu.OpenOpen.EffectBroker "" \
  "$expected_broker_unsigned_sha"
sign_owned_code \
  "$app/Contents/MacOS/OpenOpenEffectBrokerWorker" \
  com.thesongzhu.OpenOpen.EffectBroker.Worker "" \
  "$expected_worker_unsigned_sha"
sign_owned_code "$app" com.thesongzhu.OpenOpen
/usr/bin/codesign --verify --deep --strict "$app"
verify_app_modes "$app"
verify_app_operational_metadata "$app"
verify_signing_field "$app/Contents/MacOS/OpenOpenCore" Identifier \
  com.thesongzhu.OpenOpen.Core
verify_signing_field "$app/Contents/MacOS/OpenOpenEffectBroker" Identifier \
  com.thesongzhu.OpenOpen.EffectBroker
verify_signing_field "$app/Contents/MacOS/OpenOpenEffectBrokerWorker" Identifier \
  com.thesongzhu.OpenOpen.EffectBroker.Worker
verify_signing_field "$app/Contents/Resources/iMessage/0.13.0/bin/imsg" Identifier \
  com.thesongzhu.OpenOpen.imsg
imsg_automation="$({ /usr/bin/codesign -d --entitlements :- \
  "$app/Contents/Resources/iMessage/0.13.0/bin/imsg" 2>/dev/null || true; } \
  | /usr/bin/plutil -extract 'com\.apple\.security\.automation\.apple-events' raw - \
    2>/dev/null || true)"
[[ "$imsg_automation" == "true" ]] || {
  echo "signed imsg is missing its exact Apple Events entitlement" >&2
  exit 66
}
if [[ "$signing_mode" == "developer-id" ]]; then
  [[ "$imsg_team" =~ ^[A-Z0-9]{10}$ ]] || {
    echo "Developer-ID signature did not produce a valid TeamIdentifier" >&2
    exit 66
  }
  for owned in \
    "$app" \
    "$app/Contents/MacOS/OpenOpenCore" \
    "$app/Contents/MacOS/OpenOpenEffectBroker" \
    "$app/Contents/MacOS/OpenOpenEffectBrokerWorker" \
    "$app/Contents/Resources/iMessage/0.13.0/bin/imsg"; do
    verify_signing_field "$owned" TeamIdentifier "$imsg_team"
    verify_hardened_timestamped "$owned"
    verify_developer_id_application "$owned" "$imsg_team"
    verify_owner_certificate "$owned"
    verify_no_get_task_allow "$owned"
  done
  verify_signing_field "$app/Contents/Resources/Codex/0.144.0/bin/codex-code-mode-host" \
    Identifier codex-code-mode-host
  for codex_owned in \
    "$app/Contents/Resources/Codex/0.144.0/bin/codex" \
    "$app/Contents/Resources/Codex/0.144.0/bin/codex-code-mode-host"; do
    verify_signing_field "$codex_owned" TeamIdentifier 2DC432GLL2
    verify_hardened_timestamped "$codex_owned"
    verify_developer_id_application "$codex_owned" 2DC432GLL2
    verify_no_get_task_allow "$codex_owned"
  done
  verify_signing_field "$app/Contents/Resources/Codex/0.144.0/codex-path/rg" \
    Identifier rg
  verify_signing_field "$app/Contents/Resources/Codex/0.144.0/codex-path/rg" \
    TeamIdentifier "$imsg_team"
  verify_hardened_timestamped "$app/Contents/Resources/Codex/0.144.0/codex-path/rg"
  verify_developer_id_application \
    "$app/Contents/Resources/Codex/0.144.0/codex-path/rg" "$imsg_team"
  verify_owner_certificate "$app/Contents/Resources/Codex/0.144.0/codex-path/rg"
  verify_no_get_task_allow "$app/Contents/Resources/Codex/0.144.0/codex-path/rg"
fi
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
verify_app_modes "$output"
verify_app_operational_metadata "$output"
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
verify_sha "$codex_rg_runtime_sha" "$output/Contents/Resources/Codex/0.144.0/codex-path/rg"
if [[ "$signing_mode" == "developer-id" ]]; then
  verify_owner_certificate "$output"
  verify_owner_certificate "$output/Contents/MacOS/OpenOpenCore"
  verify_owner_certificate "$output/Contents/MacOS/OpenOpenEffectBroker"
  verify_owner_certificate "$output/Contents/MacOS/OpenOpenEffectBrokerWorker"
  verify_owner_certificate "$output/Contents/Resources/Codex/0.144.0/codex-path/rg"
  verify_owner_certificate "$output/Contents/Resources/iMessage/0.13.0/bin/imsg"
  verify_unsigned_macho_sha "$expected_app_unsigned_sha" "$output/Contents/MacOS/OpenOpen"
  verify_unsigned_macho_sha "$expected_core_unsigned_sha" "$output/Contents/MacOS/OpenOpenCore"
  verify_unsigned_macho_sha "$expected_broker_unsigned_sha" "$output/Contents/MacOS/OpenOpenEffectBroker"
  verify_unsigned_macho_sha "$expected_worker_unsigned_sha" "$output/Contents/MacOS/OpenOpenEffectBrokerWorker"
  verify_unsigned_macho_sha "$expected_rg_unsigned_sha" "$output/Contents/Resources/Codex/0.144.0/codex-path/rg"
  verify_unsigned_macho_sha "$expected_imsg_unsigned_sha" "$output/Contents/Resources/iMessage/0.13.0/bin/imsg"
  /usr/bin/jq -e \
    --arg upstream "$codex_rg_upstream_sha" \
    --arg runtime "$codex_rg_runtime_sha" \
    --arg team "$imsg_team" \
    '.schemaVersion == 1 and .component == "openai/codex" and .version == "0.144.0"
      and .upstreamRgSha256 == $upstream and .runtimeRgSha256 == $runtime
      and .signingIdentifier == "rg" and .teamIdentifier == $team
      and (.cdhash | length == 40)' \
    "$output/Contents/Resources/Notices/CODEX-RUNTIME-RECEIPT.json" >/dev/null
fi
claimed_output=0
if [[ "$signing_mode" == "developer-id" ]]; then
  echo "STAGED_DEVELOPER_ID_NOT_NOTARIZED_NOT_RELEASE_PROOF $imsg_team $output"
else
  echo "STAGED_AD_HOC_NOT_RELEASE_PROOF $output"
fi
