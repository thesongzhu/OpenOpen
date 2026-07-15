#!/bin/bash
set -euo pipefail
umask 077

usage() {
  echo "usage: $0 --app ABSOLUTE_PATH --output ABSOLUTE_PATH [--developer-id-identity CERTIFICATE_NAME]" >&2
  exit 64
}

app=""
output=""
signing_identity=""
readonly expected_developer_id_identity="Developer ID Application: Wenxin Dou (UHDY2275L5)"
readonly expected_developer_id_team="UHDY2275L5"
readonly expected_developer_id_leaf_sha="a7e43925d8ee4ad927f6ac27078eff554b7487a58f73b8f3acd7fabadc4057c8"
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
    --developer-id-identity)
      [[ $# -ge 2 && -n "$2" ]] || usage
      signing_identity="$2"
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

verify_sha() {
  local expected="$1"
  local path="$2"
  local actual
  actual="$(/usr/bin/shasum -a 256 "$path" | /usr/bin/awk '{print $1}')"
  [[ "$actual" == "$expected" ]] || {
    echo "exact alpha component hash mismatch: $path" >&2
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
  /usr/bin/codesign --remove-signature "$scratch" || {
    rm -f "$scratch"
    exit 66
  }
  actual="$(/usr/bin/shasum -a 256 "$scratch" | /usr/bin/awk '{print $1}')"
  rm -f "$scratch"
  [[ "$actual" == "$expected" ]] || {
    echo "exact alpha unsigned Mach-O hash mismatch: $path" >&2
    exit 66
  }
}

signing_value() {
  local path="$1"
  local field="$2"
  /usr/bin/codesign -d --verbose=4 "$path" 2>&1 \
    | /usr/bin/awk -F= -v field="$field" '$1 == field {print $2}'
}

verify_signing_value() {
  local path="$1"
  local field="$2"
  local expected="$3"
  local actual
  actual="$(signing_value "$path" "$field")"
  [[ "$actual" == "$expected" ]] || {
    echo "unexpected $field for exact alpha component $path: $actual" >&2
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
    echo "unable to extract exact alpha leaf certificate: $path" >&2
    exit 66
  }
  [[ "$actual" == "$expected_developer_id_leaf_sha" ]] || {
    echo "unexpected exact alpha Developer ID leaf certificate: $path" >&2
    exit 66
  }
}

verify_entry_operational_metadata() {
  local path="$1"
  local flags acl_lines attribute
  flags="$(/usr/bin/stat -f '%f' "$path")"
  [[ "$flags" == "0" ]] || {
    echo "exact alpha entry has forbidden BSD flags: $path" >&2
    exit 66
  }
  acl_lines="$(/bin/ls -lde "$path" | /usr/bin/wc -l | /usr/bin/tr -d ' ')"
  [[ "$acl_lines" == "1" ]] || {
    echo "exact alpha entry has a forbidden ACL: $path" >&2
    exit 66
  }
  while IFS= read -r attribute; do
    [[ -z "$attribute" || "$attribute" == "com.apple.provenance" ]] || {
      echo "exact alpha entry has a forbidden extended attribute: $path" >&2
      exit 66
    }
  done < <(/usr/bin/xattr "$path" 2>/dev/null || true)
}

verify_developer_id_code() {
  local path="$1"
  local team="$2"
  local details requirement
  details="$(/usr/bin/codesign -d --verbose=4 "$path" 2>&1)"
  /usr/bin/grep -E 'flags=.*\(runtime\)' <<<"$details" >/dev/null || {
    echo "exact alpha component lacks hardened runtime: $path" >&2
    exit 66
  }
  /usr/bin/grep -E '^Timestamp=' <<<"$details" >/dev/null || {
    echo "exact alpha component lacks secure timestamp: $path" >&2
    exit 66
  }
  requirement="=anchor apple generic and certificate leaf[field.1.2.840.113635.100.6.1.13] /* exists */ and certificate leaf[subject.OU] = \"$team\""
  /usr/bin/codesign --verify --strict --test-requirement "$requirement" "$path" || {
    echo "exact alpha component is not Developer ID Application code for Team $team: $path" >&2
    exit 66
  }
  get_task_allow="$({ /usr/bin/codesign -d --entitlements :- "$path" 2>/dev/null || true; } \
    | /usr/bin/plutil -extract 'com\.apple\.security\.get-task-allow' raw - \
      2>/dev/null || true)"
  [[ "$get_task_allow" != "true" ]] || {
    echo "get-task-allow is forbidden in exact alpha code: $path" >&2
    exit 66
  }
}

entitlements_json() {
  local path="$1"
  local raw
  raw="$({ /usr/bin/codesign -d --entitlements :- "$path" 2>/dev/null || true; })"
  if [[ -z "$raw" ]]; then
    /usr/bin/printf '{}\n'
  else
    /usr/bin/printf '%s' "$raw" | /usr/bin/plutil -convert json -o - -
  fi
}

verify_exact_developer_app() {
  local candidate="$1"
  local app_team="$2"
  local openai_team="2DC432GLL2"
  local rel path actual_macho expected_macho entitlement_json mode expected_mode
  local actual_app_files expected_app_files actual_app_dirs expected_app_dirs
  local notice_hashes runtime_sha

  [[ -d "$candidate/Contents" && ! -L "$candidate" \
    && -z "$(/usr/bin/find -P "$candidate" -type l -print -quit)" ]] || {
    echo "exact alpha app contains a missing bundle or alias" >&2
    exit 66
  }
  [[ "$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' \
    "$candidate/Contents/Info.plist")" == "com.thesongzhu.OpenOpen" ]] || {
    echo "exact alpha bundle identifier mismatch" >&2
    exit 66
  }
  verify_signing_value "$candidate" Identifier com.thesongzhu.OpenOpen
  verify_signing_value "$candidate" TeamIdentifier "$app_team"
  verify_owner_certificate "$candidate"

  expected_macho="$(/usr/bin/printf '%s\n' \
    Contents/MacOS/OpenOpen \
    Contents/MacOS/OpenOpenCore \
    Contents/MacOS/OpenOpenEffectBroker \
    Contents/MacOS/OpenOpenEffectBrokerWorker \
    Contents/Resources/Codex/0.144.0/bin/codex \
    Contents/Resources/Codex/0.144.0/bin/codex-code-mode-host \
    Contents/Resources/Codex/0.144.0/codex-path/rg \
    Contents/Resources/iMessage/0.13.0/bin/imsg | LC_ALL=C sort)"
  actual_macho="$(
    /usr/bin/find -P "$candidate" -type f -print0 \
      | while IFS= read -r -d '' path; do
          if [[ "$(/usr/bin/file -b "$path")" == *"Mach-O"* ]]; then
            /usr/bin/printf '%s\n' "${path#"$candidate"/}"
          fi
        done \
      | LC_ALL=C sort
  )"
  [[ "$actual_macho" == "$expected_macho" ]] || {
    echo "exact alpha Mach-O allowlist mismatch" >&2
    exit 66
  }

  while IFS='|' read -r rel identifier team cdhash unsigned_sha; do
    path="$candidate/$rel"
    [[ -f "$path" && ! -L "$path" ]] || {
      echo "missing exact alpha Mach-O: $rel" >&2
      exit 66
    }
    verify_signing_value "$path" Identifier "$identifier"
    verify_signing_value "$path" TeamIdentifier "$team"
    if [[ "$cdhash" != "-" ]]; then
      verify_signing_value "$path" CDHash "$cdhash"
    fi
    if [[ "$unsigned_sha" != "-" ]]; then
      verify_unsigned_macho_sha "$unsigned_sha" "$path"
    fi
    verify_developer_id_code "$path" "$team"
    if [[ "$team" == "$app_team" ]]; then
      verify_owner_certificate "$path"
    fi
  done <<EOF
Contents/MacOS/OpenOpen|com.thesongzhu.OpenOpen|$app_team|-|975e7872d07684c9763952d826fe1e3f3e48d90bf1435a48d3e24075684a5cfa
Contents/MacOS/OpenOpenCore|com.thesongzhu.OpenOpen.Core|$app_team|e95d09ccef59224a9855ff8fe2e1f8ad7994ae7e|47b4fae44d7fb2bbe089beb6db17003b4e96691d2fd120d10eaafe6a52c0c60a
Contents/MacOS/OpenOpenEffectBroker|com.thesongzhu.OpenOpen.EffectBroker|$app_team|56e34ac6b4fb2635f685e73cc7580087a0bcc4a6|24f593b7e7d9aaed4bcdd087d21f8cd7d5082c11da289ea385f78d976f9e1f12
Contents/MacOS/OpenOpenEffectBrokerWorker|com.thesongzhu.OpenOpen.EffectBroker.Worker|$app_team|13ec588bddb971a721d0e7d858d81dd64fc39a04|af9c72d3eba3adab68ddb6f6d89997f92900fb7826f3be80ad510a2007fd7d05
Contents/Resources/Codex/0.144.0/bin/codex|codex|$openai_team|cf4f00c153b0ef5af3f71281d1a6c47be9c85c8e|-
Contents/Resources/Codex/0.144.0/bin/codex-code-mode-host|codex-code-mode-host|$openai_team|3ed966beb3746263b5d22e6ba0e81f41ace50f03|-
Contents/Resources/Codex/0.144.0/codex-path/rg|rg|$app_team|b117313f07e30d05462b942c318b1ae0b73b4e5c|ea91b02e833a93bea206911bb80434a837d11a4d2eca520548abd07cece2c2c6
Contents/Resources/iMessage/0.13.0/bin/imsg|com.thesongzhu.OpenOpen.imsg|$app_team|19de2b3e834adf95fed67c0cfd1a6f6a7759d5de|cdea42cf30e731d52c00524c16db5865fe2d01ef6a3f377cb6e3a3eb65f5f313
EOF

  for rel in \
    Contents/MacOS/OpenOpen \
    Contents/MacOS/OpenOpenCore \
    Contents/MacOS/OpenOpenEffectBroker \
    Contents/MacOS/OpenOpenEffectBrokerWorker \
    Contents/Resources/Codex/0.144.0/codex-path/rg; do
    [[ "$(entitlements_json "$candidate/$rel")" == "{}" ]] || {
      echo "unexpected entitlement on exact alpha component: $rel" >&2
      exit 66
    }
  done
  for rel in \
    Contents/Resources/Codex/0.144.0/bin/codex \
    Contents/Resources/Codex/0.144.0/bin/codex-code-mode-host; do
    entitlement_json="$(entitlements_json "$candidate/$rel")"
    /usr/bin/jq -e \
      'keys == ["com.apple.security.cs.allow-jit", "com.apple.security.cs.allow-unsigned-executable-memory"]
       and .["com.apple.security.cs.allow-jit"] == true
       and .["com.apple.security.cs.allow-unsigned-executable-memory"] == true' \
      <<<"$entitlement_json" >/dev/null || {
      echo "unexpected exact Codex entitlement set: $rel" >&2
      exit 66
    }
  done
  entitlement_json="$(entitlements_json \
    "$candidate/Contents/Resources/iMessage/0.13.0/bin/imsg")"
  /usr/bin/jq -e \
    'keys == ["com.apple.security.automation.apple-events"]
     and .["com.apple.security.automation.apple-events"] == true' \
    <<<"$entitlement_json" >/dev/null || {
    echo "unexpected exact imsg entitlement set" >&2
    exit 66
  }

  verify_sha c1769b4093faa6e8bde56cdb16ad2c950ee39ea5501630e0ba022901b56a7b3d \
    "$candidate/Contents/Resources/iMessage/0.13.0/BUILD-RECEIPT.json"
  verify_sha 818495226dda3332f711fc6d6408eacf1776e08fcddfa06342ab3f5196417839 \
    "$candidate/Contents/Resources/Notices/third_party/manifest.json"
  verify_sha 1046fc149ee4a2e607ed91afa48d1ab98a98ab62f7e8acd57b67f44e8f3b13bd \
    "$candidate/Contents/Resources/Notices/PROVENANCE.md"
  verify_sha c1b3af67fd28bbf768765357251f7b29d315150068cb41101dc77eb8a42bc7eb \
    "$candidate/Contents/Resources/Codex/0.144.0/codex-package.json"
  verify_sha 978740e6bcbd9af2f850823b723fb74f16d8d1e44de05f7dd6737ae631f72017 \
    "$candidate/Contents/Resources/Codex/0.144.0/bin/codex"
  verify_sha 067ee7894c49489ca72fc2ca6093f408302241bd22097fcd12d785c9ba40fd43 \
    "$candidate/Contents/Resources/Codex/0.144.0/bin/codex-code-mode-host"
  verify_sha 392daa0f1b1ab81e1a974299320f5f243220900858c5824d9a11aa5307517442 \
    "$candidate/Contents/Info.plist"
  verify_sha fc061f73b2490648d1bffe58a1342fe8d41b2a640093317f6aad07c8b674ba3a \
    "$candidate/Contents/Library/LaunchDaemons/com.thesongzhu.OpenOpen.EffectBroker.plist"
  verify_sha 8c2dce8fc7a250d57b6b1583acb3033c66769e5281151953cbf139539aca11ca \
    "$candidate/Contents/Resources/Notices/THIRD_PARTY_NOTICES.md"
  verify_sha 8ca856c3507586427462bfb2dfcfd3dfb94070e40efc272430b88481e71ba283 \
    "$candidate/Contents/Resources/iMessage/0.13.0/bin/PhoneNumberKit_PhoneNumberKit.bundle/PhoneNumberMetadata.json"
  verify_sha 561040f7a52952f75d02d4b6758382ff48c563d6f25c84a09f1a655f6dc60ff8 \
    "$candidate/Contents/Resources/iMessage/0.13.0/bin/PhoneNumberKit_PhoneNumberKit.bundle/PrivacyInfo.xcprivacy"

  actual_imsg_files="$(
    cd "$candidate/Contents/Resources/iMessage/0.13.0"
    /usr/bin/find -P . -type f -print | /usr/bin/sed 's#^./##' | LC_ALL=C sort
  )"
  expected_imsg_files="$(/usr/bin/printf '%s\n' \
    BUILD-RECEIPT.json \
    RUNTIME-RECEIPT.json \
    bin/PhoneNumberKit_PhoneNumberKit.bundle/PhoneNumberMetadata.json \
    bin/PhoneNumberKit_PhoneNumberKit.bundle/PrivacyInfo.xcprivacy \
    bin/imsg | LC_ALL=C sort)"
  [[ "$actual_imsg_files" == "$expected_imsg_files" ]] || {
    echo "exact staged imsg runtime allowlist mismatch" >&2
    exit 66
  }

  notice_hashes="$(/usr/bin/jq -r '.. | objects | .textSha256? // empty' \
    "$candidate/Contents/Resources/Notices/third_party/manifest.json" \
    | LC_ALL=C sort -u)"
  [[ "$(/usr/bin/printf '%s\n' "$notice_hashes" | /usr/bin/awk 'NF {count++} END {print count+0}')" \
    == "597" ]] || {
    echo "exact notice hash closure count mismatch" >&2
    exit 66
  }
  while IFS= read -r notice_hash; do
    verify_sha "$notice_hash" \
      "$candidate/Contents/Resources/Notices/third_party/texts/$notice_hash.txt"
  done <<<"$notice_hashes"

  expected_app_files="$(
    {
      /usr/bin/printf '%s\n' \
        Contents/Info.plist \
        Contents/Library/LaunchDaemons/com.thesongzhu.OpenOpen.EffectBroker.plist \
        Contents/MacOS/OpenOpen \
        Contents/MacOS/OpenOpenCore \
        Contents/MacOS/OpenOpenEffectBroker \
        Contents/MacOS/OpenOpenEffectBrokerWorker \
        Contents/Resources/Codex/0.144.0/bin/codex \
        Contents/Resources/Codex/0.144.0/bin/codex-code-mode-host \
        Contents/Resources/Codex/0.144.0/codex-package.json \
        Contents/Resources/Codex/0.144.0/codex-path/rg \
        Contents/Resources/Notices/CODEX-RUNTIME-RECEIPT.json \
        Contents/Resources/Notices/PROVENANCE.md \
        Contents/Resources/Notices/THIRD_PARTY_NOTICES.md \
        Contents/Resources/Notices/third_party/manifest.json \
        Contents/Resources/iMessage/0.13.0/BUILD-RECEIPT.json \
        Contents/Resources/iMessage/0.13.0/RUNTIME-RECEIPT.json \
        Contents/Resources/iMessage/0.13.0/bin/PhoneNumberKit_PhoneNumberKit.bundle/PhoneNumberMetadata.json \
        Contents/Resources/iMessage/0.13.0/bin/PhoneNumberKit_PhoneNumberKit.bundle/PrivacyInfo.xcprivacy \
        Contents/Resources/iMessage/0.13.0/bin/imsg \
        Contents/_CodeSignature/CodeResources
      while IFS= read -r notice_hash; do
        /usr/bin/printf 'Contents/Resources/Notices/third_party/texts/%s.txt\n' \
          "$notice_hash"
      done <<<"$notice_hashes"
    } | LC_ALL=C sort
  )"
  actual_app_files="$(
    cd "$candidate"
    /usr/bin/find -P . -type f -print | /usr/bin/sed 's#^./##' | LC_ALL=C sort
  )"
  [[ "$actual_app_files" == "$expected_app_files" ]] || {
    echo "exact alpha App file allowlist mismatch" >&2
    exit 66
  }
  [[ -z "$(/usr/bin/find -P "$candidate" ! -type d ! -type f -print -quit)" ]] || {
    echo "exact alpha App contains a non-regular entry" >&2
    exit 66
  }

  expected_app_dirs="$(/usr/bin/printf '%s\n' \
    . \
    Contents \
    Contents/Library \
    Contents/Library/LaunchDaemons \
    Contents/MacOS \
    Contents/Resources \
    Contents/Resources/Codex \
    Contents/Resources/Codex/0.144.0 \
    Contents/Resources/Codex/0.144.0/bin \
    Contents/Resources/Codex/0.144.0/codex-path \
    Contents/Resources/Notices \
    Contents/Resources/Notices/third_party \
    Contents/Resources/Notices/third_party/texts \
    Contents/Resources/iMessage \
    Contents/Resources/iMessage/0.13.0 \
    Contents/Resources/iMessage/0.13.0/bin \
    Contents/Resources/iMessage/0.13.0/bin/PhoneNumberKit_PhoneNumberKit.bundle \
    Contents/_CodeSignature | LC_ALL=C sort)"
  actual_app_dirs="$(
    cd "$candidate"
    /usr/bin/find -P . -type d -print | /usr/bin/sed 's#^\./##; s#^$#.#' | LC_ALL=C sort
  )"
  [[ "$actual_app_dirs" == "$expected_app_dirs" ]] || {
    echo "exact alpha App directory allowlist mismatch" >&2
    exit 66
  }
  while IFS= read -r rel; do
    mode="$(/usr/bin/stat -f '%Lp' "$candidate/$rel")"
    [[ "$mode" == "755" ]] || {
      echo "exact alpha directory mode mismatch: $rel" >&2
      exit 66
    }
    verify_entry_operational_metadata "$candidate/$rel"
  done <<<"$expected_app_dirs"
  while IFS= read -r rel; do
    expected_mode="644"
    if /usr/bin/grep -Fx -- "$rel" <<<"$expected_macho" >/dev/null; then
      expected_mode="755"
    fi
    mode="$(/usr/bin/stat -f '%Lp' "$candidate/$rel")"
    [[ "$mode" == "$expected_mode" ]] || {
      echo "exact alpha file mode mismatch: $rel" >&2
      exit 66
    }
    verify_entry_operational_metadata "$candidate/$rel"
  done <<<"$expected_app_files"

  runtime_sha="$(/usr/bin/shasum -a 256 \
    "$candidate/Contents/Resources/iMessage/0.13.0/bin/imsg" \
    | /usr/bin/awk '{print $1}')"
  /usr/bin/jq -e --arg team "$app_team" \
    --arg runtime_sha "$runtime_sha" \
    '.schemaVersion == 1
     and .buildReceiptSha256 == "c1769b4093faa6e8bde56cdb16ad2c950ee39ea5501630e0ba022901b56a7b3d"
     and .binarySha256 == $runtime_sha
     and .resourceTreeSha256 == "7a5cb869823a893a7181bcacfef6dfc8be335a5ce2bf14caac579096f78909cc"
     and .signingIdentifier == "com.thesongzhu.OpenOpen.imsg"
     and .teamIdentifier == $team
     and .cdhash == "19de2b3e834adf95fed67c0cfd1a6f6a7759d5de"' \
    "$candidate/Contents/Resources/iMessage/0.13.0/RUNTIME-RECEIPT.json" >/dev/null || {
    echo "exact imsg runtime receipt mismatch" >&2
    exit 66
  }
  runtime_sha="$(/usr/bin/shasum -a 256 \
    "$candidate/Contents/Resources/Codex/0.144.0/codex-path/rg" \
    | /usr/bin/awk '{print $1}')"
  /usr/bin/jq -e --arg team "$app_team" \
    --arg runtime_sha "$runtime_sha" \
    '.schemaVersion == 1 and .component == "openai/codex" and .version == "0.144.0"
     and .upstreamRgSha256 == "4fdf1d8365af224bc70e3c1490d8461d859c37cc70e739a11e987af0215f3e94"
     and .runtimeRgSha256 == $runtime_sha
     and .signingIdentifier == "rg" and .teamIdentifier == $team
     and .cdhash == "b117313f07e30d05462b942c318b1ae0b73b4e5c"' \
    "$candidate/Contents/Resources/Notices/CODEX-RUNTIME-RECEIPT.json" >/dev/null || {
    echo "exact Codex runtime receipt mismatch" >&2
    exit 66
  }

  /usr/bin/codesign --verify --deep --strict "$candidate"
  verify_developer_id_code "$candidate" "$app_team"
}

/usr/bin/codesign --verify --deep --strict "$app"
app_team="$(/usr/bin/codesign -d --verbose=4 "$app" 2>&1 \
  | /usr/bin/awk -F= '$1 == "TeamIdentifier" {print $2}')"
if [[ -n "$signing_identity" ]]; then
  [[ "$signing_identity" == "$expected_developer_id_identity" ]] || {
    echo "DMG signing requires the pinned owner Developer ID Application certificate" >&2
    exit 65
  }
  [[ "$app_team" == "$expected_developer_id_team" ]] || {
    echo "refusing to sign a DMG whose app lacks the pinned owner TeamIdentifier" >&2
    exit 65
  }
  app_requirement="=anchor apple generic and certificate leaf[field.1.2.840.113635.100.6.1.13] /* exists */ and certificate leaf[subject.OU] = \"$app_team\""
  /usr/bin/codesign --verify --strict --test-requirement "$app_requirement" "$app" || {
    echo "app is not signed by an Apple Developer ID Application for Team $app_team" >&2
    exit 65
  }
  verify_owner_certificate "$app"
  /usr/bin/security find-identity -v -p codesigning \
    | /usr/bin/grep -F -- "\"$signing_identity\"" >/dev/null || {
      echo "requested Developer ID Application identity is unavailable" >&2
      exit 65
    }
  verify_exact_developer_app "$app" "$app_team"
fi

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
if [[ -n "$signing_identity" ]]; then
  /usr/bin/codesign --force --sign "$signing_identity" --timestamp "$output"
  /usr/bin/codesign --verify --strict "$output"
  dmg_team="$(/usr/bin/codesign -d --verbose=4 "$output" 2>&1 \
    | /usr/bin/awk -F= '$1 == "TeamIdentifier" {print $2}')"
  [[ "$dmg_team" == "$app_team" ]] || {
    echo "DMG and app TeamIdentifier values differ" >&2
    exit 66
  }
  /usr/bin/codesign --verify --strict --test-requirement "$app_requirement" "$output" || {
    echo "DMG is not signed by the app Team Developer ID Application" >&2
    exit 66
  }
  verify_owner_certificate "$output"
  dmg_details="$(/usr/bin/codesign -d --verbose=4 "$output" 2>&1)"
  /usr/bin/grep -E '^Timestamp=' <<<"$dmg_details" >/dev/null || {
    echo "signed DMG lacks a secure timestamp" >&2
    exit 66
  }
fi

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
if [[ -n "$signing_identity" ]]; then
  verify_exact_developer_app "$mountpoint/OpenOpen.app" "$app_team"
fi
/usr/bin/ditto "$mountpoint/OpenOpen.app" "$install_root/OpenOpen.app"
/usr/bin/codesign --verify --deep --strict "$install_root/OpenOpen.app"
if [[ -n "$signing_identity" ]]; then
  verify_exact_developer_app "$install_root/OpenOpen.app" "$app_team"
fi
/usr/bin/hdiutil detach "$mountpoint" -quiet
mounted=0

dmg_sha="$(/usr/bin/shasum -a 256 "$output" | /usr/bin/awk '{print $1}')"
claimed_output=0
if [[ -n "$signing_identity" ]]; then
  echo "ALPHA_DMG_DEVELOPER_ID_NOT_NOTARIZED_NOT_RELEASE_PROOF $app_team $dmg_sha $output"
else
  echo "ALPHA_DMG_NOT_NOTARIZED_NOT_RELEASE_PROOF $dmg_sha $output"
fi
