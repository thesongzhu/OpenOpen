#!/bin/bash
set -euo pipefail

usage() {
  echo "usage: $0 --codex-package-root ABSOLUTE_PATH --imsg-binary ABSOLUTE_PATH --imsg-receipt ABSOLUTE_PATH --output ABSOLUTE_PATH [--developer-id-identity CERTIFICATE_NAME --identity-receipt-output ABSOLUTE_PATH]" >&2
  exit 64
}

codex_root=""
imsg_binary=""
imsg_receipt=""
output=""
identity_receipt_output=""
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
    --identity-receipt-output)
      [[ $# -ge 2 ]] || usage
      identity_receipt_output="$2"
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
  [[ "$identity_receipt_output" = /* \
    && "$identity_receipt_output" == *.receipt \
    && "$identity_receipt_output" != "/" ]] || {
    echo "Developer-ID mode requires an absolute external .receipt output" >&2
    exit 65
  }
  [[ ! -e "$identity_receipt_output" ]] || {
    echo "refusing to overwrite existing identity receipt: $identity_receipt_output" >&2
    exit 65
  }
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
if [[ "$signing_mode" != "developer-id" && -n "$identity_receipt_output" ]]; then
  echo "post-stage identity receipts require Developer-ID mode" >&2
  exit 65
fi
codex_root="$(cd "$codex_root" && pwd -P)"
output_parent="$(dirname "$output")"
mkdir -p "$output_parent"
output_parent="$(cd "$output_parent" && pwd -P)"
output="$output_parent/$(basename "$output")"
if [[ -n "$identity_receipt_output" ]]; then
  identity_receipt_parent="$(dirname "$identity_receipt_output")"
  mkdir -p "$identity_receipt_parent"
  identity_receipt_parent="$(cd "$identity_receipt_parent" && pwd -P)"
  identity_receipt_output="$identity_receipt_parent/$(basename "$identity_receipt_output")"
fi

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
    echo "unsigned Mach-O content mismatch: $path (expected $expected, actual $actual)" >&2
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

signing_value() {
  local path="$1"
  local field="$2"
  /usr/bin/codesign -d --verbose=4 "$path" 2>&1 \
    | /usr/bin/awk -F= -v field="$field" '$1 == field {print $2}'
}

canonical_xattrs() {
  local path="$1"
  local attribute value result=""
  while IFS= read -r attribute; do
    [[ -n "$attribute" ]] || continue
    value="$(/usr/bin/xattr -px "$attribute" "$path" 2>/dev/null \
      | /usr/bin/tr -d ' \r\n')"
    result+="${attribute}=${value};"
  done < <(/usr/bin/xattr "$path" 2>/dev/null | LC_ALL=C sort || true)
  if [[ -n "$result" ]]; then
    /usr/bin/printf '%s\n' "$result"
  else
    /usr/bin/printf '%s\n' '-'
  fi
}

write_tree_manifest() {
  local root="$1"
  local destination="$2"
  local path relative kind mode flags attributes size sha
  [[ -d "$root" && ! -L "$root" \
    && -z "$(/usr/bin/find -P "$root" ! -type d ! -type f -print -quit)" ]] || {
    echo "manifest root contains a missing, aliased, or non-regular entry: $root" >&2
    exit 66
  }
  (
    /usr/bin/printf 'OPENOPEN-TREE-MANIFEST-V1\n'
    while IFS= read -r -d '' path; do
      relative="${path#"$root"/}"
      [[ "$path" != "$root" ]] || relative="."
      if LC_ALL=C /usr/bin/printf '%s' "$relative" \
        | /usr/bin/grep -q '[[:cntrl:]]'; then
        echo "manifest path contains a forbidden control byte" >&2
        return 66
      fi
      mode="$(/usr/bin/stat -f '%Lp' "$path")"
      flags="$(/usr/bin/stat -f '%f' "$path")"
      attributes="$(canonical_xattrs "$path")"
      if [[ -d "$path" ]]; then
        kind="D"
        size="-"
        sha="-"
      else
        kind="F"
        size="$(/usr/bin/stat -f '%z' "$path")"
        sha="$(/usr/bin/shasum -a 256 "$path" | /usr/bin/awk '{print $1}')"
      fi
      /usr/bin/printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
        "$kind" "$mode" "$flags" "$attributes" "$size" "$sha" "$relative"
    done < <(/usr/bin/find -P "$root" -print0 | LC_ALL=C /usr/bin/sort -z)
  ) >"$destination"
}

write_source_snapshot_manifest() {
  local source_root="$1"
  local destination="$2"
  local scratch index_raw index_raw_after flags_raw flags_raw_after head_raw
  local index_manifest head_manifest index_manifest_sha
  local head diff_sha record metadata relative path
  local index_mode index_oid index_stage extra tag
  local tree_mode tree_type tree_oid mode mode_value size size_after sha
  scratch="$(/usr/bin/mktemp -d /private/tmp/OpenOpen-source-index.XXXXXX)"
  index_raw="$scratch/index.raw"
  index_raw_after="$scratch/index-after.raw"
  flags_raw="$scratch/index-flags.raw"
  flags_raw_after="$scratch/index-flags-after.raw"
  head_raw="$scratch/head.raw"
  index_manifest="$scratch/index.tsv"
  head_manifest="$scratch/head.tsv"
  git -C "$source_root" diff --cached --quiet -- || {
    rm -rf "$scratch"
    echo "source snapshot requires an empty Git index" >&2
    return 66
  }
  head="$(git -C "$source_root" rev-parse HEAD)"
  git -C "$source_root" ls-files --stage -z >"$index_raw"
  git -C "$source_root" ls-files -v -z --cached >"$flags_raw"
  git -C "$source_root" ls-tree -r -z --full-tree "$head" >"$head_raw"
  git -C "$source_root" ls-files --stage -z >"$index_raw_after"
  git -C "$source_root" ls-files -v -z --cached >"$flags_raw_after"
  if ! /usr/bin/cmp -s "$index_raw" "$index_raw_after" \
    || ! /usr/bin/cmp -s "$flags_raw" "$flags_raw_after"; then
    rm -rf "$scratch"
    echo "source Git index changed while its snapshot was captured" >&2
    return 66
  fi
  while IFS= read -r -d '' record; do
    [[ "$record" == ?' '* ]] || {
      rm -rf "$scratch"
      echo "source Git index flags have an invalid shape" >&2
      return 66
    }
    tag="${record%% *}"
    relative="${record#? }"
    [[ "$tag" == "H" && -n "$relative" ]] || {
      rm -rf "$scratch"
      echo "source Git index contains assume-unchanged, skip-worktree, or non-cached state: $relative" >&2
      return 66
    }
  done <"$flags_raw"
  : >"$index_manifest"
  while IFS= read -r -d '' record; do
    [[ "$record" == *$'\t'* ]] || {
      rm -rf "$scratch"
      echo "source Git index entry has an invalid shape" >&2
      return 66
    }
    metadata="${record%%$'\t'*}"
    relative="${record#*$'\t'}"
    IFS=' ' read -r index_mode index_oid index_stage extra <<<"$metadata"
    [[ -z "${extra:-}" && "$index_stage" == "0" \
      && "$index_mode" =~ ^100(644|755)$ \
      && "$index_oid" =~ ^[0-9a-f]{40,64}$ ]] || {
      rm -rf "$scratch"
      echo "source Git index contains a non-regular or non-stage-zero entry: $relative" >&2
      return 66
    }
    if [[ -z "$relative" ]] \
      || LC_ALL=C /usr/bin/printf '%s' "$relative" \
        | /usr/bin/grep -q '[[:cntrl:]]'; then
      rm -rf "$scratch"
      echo "source Git index path contains a forbidden control byte" >&2
      return 66
    fi
    /usr/bin/printf '%s\t%s\t%s\n' \
      "$index_mode" "$index_oid" "$relative" >>"$index_manifest"
  done <"$index_raw"
  : >"$head_manifest"
  while IFS= read -r -d '' record; do
    [[ "$record" == *$'\t'* ]] || {
      rm -rf "$scratch"
      echo "source HEAD tree entry has an invalid shape" >&2
      return 66
    }
    metadata="${record%%$'\t'*}"
    relative="${record#*$'\t'}"
    IFS=' ' read -r tree_mode tree_type tree_oid extra <<<"$metadata"
    [[ -z "${extra:-}" && "$tree_type" == "blob" \
      && "$tree_mode" =~ ^100(644|755)$ \
      && "$tree_oid" =~ ^[0-9a-f]{40,64}$ ]] || {
      rm -rf "$scratch"
      echo "source HEAD contains a symlink, submodule, or non-regular entry: $relative" >&2
      return 66
    }
    if [[ -z "$relative" ]] \
      || LC_ALL=C /usr/bin/printf '%s' "$relative" \
        | /usr/bin/grep -q '[[:cntrl:]]'; then
      rm -rf "$scratch"
      echo "source HEAD path contains a forbidden control byte" >&2
      return 66
    fi
    /usr/bin/printf '%s\t%s\t%s\n' \
      "$tree_mode" "$tree_oid" "$relative" >>"$head_manifest"
  done <"$head_raw"
  /usr/bin/cmp -s "$index_manifest" "$head_manifest" || {
    rm -rf "$scratch"
    echo "source Git index tree does not exactly equal HEAD" >&2
    return 66
  }
  index_manifest_sha="$(/usr/bin/shasum -a 256 "$index_manifest" \
    | /usr/bin/awk '{print $1}')"
  diff_sha="$(git -C "$source_root" diff --binary --no-ext-diff -- \
    | /usr/bin/shasum -a 256 | /usr/bin/awk '{print $1}')"
  (
    /usr/bin/printf 'OPENOPEN-SOURCE-SNAPSHOT-V2\n'
    /usr/bin/printf 'HEAD\t%s\n' "$head"
    /usr/bin/printf 'INDEX\tempty\n'
    /usr/bin/printf 'INDEX-MANIFEST-SHA256\t%s\n' "$index_manifest_sha"
    /usr/bin/printf 'TRACKED-WORKTREE-DIFF-SHA256\t%s\n' "$diff_sha"
    while IFS=$'\t' read -r index_mode index_oid relative; do
      path="$source_root/$relative"
      [[ -f "$path" && ! -L "$path" ]] || {
        echo "tracked source snapshot entry is not a regular file: $relative" >&2
        return 66
      }
      mode="$(/usr/bin/stat -f '%Lp' "$path")"
      mode_value=$((8#$mode))
      size="$(/usr/bin/stat -f '%z' "$path")"
      sha="$(/usr/bin/shasum -a 256 "$path" | /usr/bin/awk '{print $1}')"
      size_after="$(/usr/bin/stat -f '%z' "$path")"
      [[ "$size" == "$size_after" ]] || {
        echo "tracked source entry changed while hashing: $relative" >&2
        return 66
      }
      if [[ "$index_mode" == "100644" ]]; then
        (( (mode_value & 0111) == 0 )) || {
          echo "tracked non-executable source has executable mode: $relative" >&2
          return 66
        }
      else
        (( (mode_value & 0100) != 0 )) || {
          echo "tracked executable source lacks owner-execute mode: $relative" >&2
          return 66
        }
      fi
      /usr/bin/printf 'TRACKED\t%s\t%s\t%s\t%s\t%s\t%s\n' \
        "$index_mode" "$index_oid" "$mode" "$size" "$sha" "$relative"
    done <"$index_manifest"
    while IFS= read -r -d '' relative; do
      if [[ -z "$relative" ]] \
        || LC_ALL=C /usr/bin/printf '%s' "$relative" \
          | /usr/bin/grep -q '[[:cntrl:]]'; then
        echo "untracked source path contains a forbidden control byte" >&2
        return 66
      fi
      path="$source_root/$relative"
      [[ -f "$path" && ! -L "$path" ]] || {
        echo "untracked source snapshot entry is not a regular file: $relative" >&2
        return 66
      }
      mode="$(/usr/bin/stat -f '%Lp' "$path")"
      size="$(/usr/bin/stat -f '%z' "$path")"
      sha="$(/usr/bin/shasum -a 256 "$path" | /usr/bin/awk '{print $1}')"
      size_after="$(/usr/bin/stat -f '%z' "$path")"
      [[ "$size" == "$size_after" ]] || {
        echo "untracked source entry changed while hashing: $relative" >&2
        return 66
      }
      /usr/bin/printf 'UNTRACKED\t%s\t%s\t%s\t%s\n' \
        "$mode" "$size" "$sha" "$relative"
    done < <(git -C "$source_root" ls-files --others --exclude-standard -z)
  ) >"$destination" || {
    rm -rf "$scratch" "$destination"
    return 66
  }
  if ! git -C "$source_root" diff --cached --quiet -- \
    || [[ "$(git -C "$source_root" rev-parse HEAD)" != "$head" ]] \
    || ! /usr/bin/cmp -s "$index_raw" \
      <(git -C "$source_root" ls-files --stage -z) \
    || ! /usr/bin/cmp -s "$flags_raw" \
      <(git -C "$source_root" ls-files -v -z --cached); then
    rm -rf "$scratch" "$destination"
    echo "source HEAD or Git index changed after its snapshot was captured" >&2
    return 66
  fi
  rm -rf "$scratch"
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
readonly expected_app_unsigned_before_sha="0518f4cb2ffb30211892d55fd7475fbfee40e1a056b10c257fa61d69fcdf9027"
readonly expected_app_unsigned_sha="d6ea4e29d47570442d8bc3518ce0a7310b63abe80d5dc94f727e8107aafebe3c"
readonly expected_core_unsigned_before_sha="63602d67ebb952ba02228ad5dc4df8517a4fb4ea4ffd6a5facc20ba7edd3e714"
readonly expected_core_unsigned_sha="160bf60550d9e7b89924d872f5b605557502d6004d775a4949146fb588359fed"
readonly expected_broker_unsigned_before_sha="07d566ba598dad22194372ba16c8f49b97f18e69e3b67d179d2dad276d7c5cf9"
readonly expected_broker_unsigned_sha="c01e0c2707ed47c86bb3f73622f7add5e2e35c5a92f981347b2c525e3f58c8ee"
readonly expected_worker_unsigned_sha="4b5b973d3d5ba1f14bb789e96f69d54766fa58e15e6784d9a9eff509f972c208"
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

staging="$(/usr/bin/mktemp -d "$output_parent/.OpenOpen-stage.XXXXXX")"
claimed_output=0
claimed_receipt=0
cleanup() {
  rm -rf "$staging"
  if [[ "$claimed_output" -eq 1 ]]; then
    rm -rf "$output"
  fi
  if [[ "$claimed_receipt" -eq 1 ]]; then
    rm -rf "$identity_receipt_output"
  fi
}
trap cleanup EXIT
prebuild_source_snapshot="$staging/prebuild-source-snapshot.tsv"
write_source_snapshot_manifest "$repo_root" "$prebuild_source_snapshot"

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
  "$expected_app_unsigned_before_sha" \
  "$expected_app_unsigned_sha"
sign_owned_code \
  "$app/Contents/MacOS/OpenOpenCore" com.thesongzhu.OpenOpen.Core "" \
  "$expected_core_unsigned_before_sha" \
  "$expected_core_unsigned_sha"
sign_owned_code \
  "$app/Contents/MacOS/OpenOpenEffectBroker" \
  com.thesongzhu.OpenOpen.EffectBroker "" \
  "$expected_broker_unsigned_before_sha" \
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
if [[ "$signing_mode" == "developer-id" ]]; then
  receipt_staging="$staging/OpenOpen-PostStage-Identity.receipt"
  receipt_resources="$receipt_staging/Contents/Resources"
  mkdir -p "$receipt_resources"
  /usr/bin/plutil -create xml1 "$receipt_staging/Contents/Info.plist"
  /usr/bin/plutil -insert CFBundleIdentifier -string \
    com.thesongzhu.OpenOpen.PostStageIdentityReceipt \
    "$receipt_staging/Contents/Info.plist"
  /usr/bin/plutil -insert CFBundleName -string OpenOpenPostStageIdentityReceipt \
    "$receipt_staging/Contents/Info.plist"
  /usr/bin/plutil -insert CFBundlePackageType -string BNDL \
    "$receipt_staging/Contents/Info.plist"
  /usr/bin/plutil -insert CFBundleVersion -string 2 \
    "$receipt_staging/Contents/Info.plist"

  app_manifest="$receipt_resources/app-manifest.tsv"
  write_tree_manifest "$output" "$app_manifest"
  app_manifest_sha="$(/usr/bin/shasum -a 256 "$app_manifest" \
    | /usr/bin/awk '{print $1}')"
  app_directory_count="$(/usr/bin/find -P "$output" -type d -print \
    | /usr/bin/wc -l | /usr/bin/tr -d ' ')"
  app_file_count="$(/usr/bin/find -P "$output" -type f -print \
    | /usr/bin/wc -l | /usr/bin/tr -d ' ')"
  [[ "$app_directory_count" == "18" && "$app_file_count" == "617" ]] || {
    echo "post-stage App shape changed before identity receipt" >&2
    exit 66
  }
  source_snapshot="$receipt_resources/source-snapshot.tsv"
  write_source_snapshot_manifest "$repo_root" "$source_snapshot"
  [[ "$(/usr/bin/head -n 1 "$source_snapshot")" \
    == "OPENOPEN-SOURCE-SNAPSHOT-V2" ]] || {
    echo "post-stage source snapshot format mismatch" >&2
    exit 66
  }
  /usr/bin/cmp -s "$prebuild_source_snapshot" "$source_snapshot" || {
    echo "source snapshot changed between build input and post-stage receipt" >&2
    exit 66
  }
  source_snapshot_sha="$(/usr/bin/shasum -a 256 "$source_snapshot" \
    | /usr/bin/awk '{print $1}')"

  components_jsonl="$staging/receipt-components.jsonl"
  : >"$components_jsonl"
  while IFS='|' read -r relative identifier unsigned_sha; do
    component="$output/$relative"
    component_team="$(signing_value "$component" TeamIdentifier)"
    component_cdhash="$(signing_value "$component" CDHash)"
    component_signed_sha="$(/usr/bin/shasum -a 256 "$component" \
      | /usr/bin/awk '{print $1}')"
    component_entitlements="$(entitlements_json "$component")"
    [[ "$component_team" == "$imsg_team" \
      && "$component_cdhash" =~ ^[0-9a-f]{40}$ \
      && "$component_entitlements" == "{}" ]] || {
      echo "post-stage component identity is invalid: $relative" >&2
      exit 66
    }
    /usr/bin/jq -n \
      --arg path "$relative" \
      --arg identifier "$identifier" \
      --arg team_identifier "$component_team" \
      --arg cdhash "$component_cdhash" \
      --arg signed_sha256 "$component_signed_sha" \
      --arg unsigned_sha256 "$unsigned_sha" \
      --argjson entitlements "$component_entitlements" \
      '{path: $path, identifier: $identifier,
        teamIdentifier: $team_identifier, cdhash: $cdhash,
        signedSha256: $signed_sha256, unsignedSha256: $unsigned_sha256,
        hardenedRuntime: true, secureTimestamp: true,
        entitlements: $entitlements}' \
      >>"$components_jsonl"
  done <<EOF
Contents/MacOS/OpenOpen|com.thesongzhu.OpenOpen|$expected_app_unsigned_sha
Contents/MacOS/OpenOpenCore|com.thesongzhu.OpenOpen.Core|$expected_core_unsigned_sha
Contents/MacOS/OpenOpenEffectBroker|com.thesongzhu.OpenOpen.EffectBroker|$expected_broker_unsigned_sha
Contents/MacOS/OpenOpenEffectBrokerWorker|com.thesongzhu.OpenOpen.EffectBroker.Worker|$expected_worker_unsigned_sha
EOF

  source_head="$(git rev-parse HEAD)"
  source_status_sha="$(git status --porcelain=v1 -uall \
    | /usr/bin/shasum -a 256 | /usr/bin/awk '{print $1}')"
  source_diff_sha="$(git diff --binary --no-ext-diff \
    | /usr/bin/shasum -a 256 | /usr/bin/awk '{print $1}')"
  stage_script_sha="$(/usr/bin/shasum -a 256 "$repo_root/scripts/stage_openopen_app.sh" \
    | /usr/bin/awk '{print $1}')"
  dmg_script_sha="$(/usr/bin/shasum -a 256 "$repo_root/scripts/create_alpha_dmg.sh" \
    | /usr/bin/awk '{print $1}')"
  provenance_sha="$(/usr/bin/shasum -a 256 "$repo_root/PROVENANCE.md" \
    | /usr/bin/awk '{print $1}')"
  third_party_notices_sha="$(/usr/bin/shasum -a 256 \
    "$repo_root/THIRD_PARTY_NOTICES.md" | /usr/bin/awk '{print $1}')"
  identity_json="$receipt_resources/identity.json"
  /usr/bin/jq -n \
    --arg receipt_kind "com.thesongzhu.OpenOpen.post-stage-identity" \
    --arg head "$source_head" \
    --arg status_sha256 "$source_status_sha" \
    --arg binary_diff_sha256 "$source_diff_sha" \
    --arg stage_script_sha256 "$stage_script_sha" \
    --arg dmg_script_sha256 "$dmg_script_sha" \
    --arg provenance_sha256 "$provenance_sha" \
    --arg third_party_notices_sha256 "$third_party_notices_sha" \
    --arg source_snapshot_sha256 "$source_snapshot_sha" \
    --arg signing_identity "$signing_identity" \
    --arg team_identifier "$imsg_team" \
    --arg leaf_certificate_sha256 "$expected_developer_id_leaf_sha" \
    --arg manifest_sha256 "$app_manifest_sha" \
    --argjson directory_count "$app_directory_count" \
    --argjson file_count "$app_file_count" \
    --slurpfile components "$components_jsonl" \
    '{schemaVersion: 2, receiptKind: $receipt_kind,
      source: {head: $head, statusSha256: $status_sha256,
        binaryDiffSha256: $binary_diff_sha256,
        indexState: "empty",
        sourceSnapshotFile: "Contents/Resources/source-snapshot.tsv",
        sourceSnapshotFormat: "OPENOPEN-SOURCE-SNAPSHOT-V2",
        sourceSnapshotSha256: $source_snapshot_sha256,
        stageScriptSha256: $stage_script_sha256,
        dmgScriptSha256: $dmg_script_sha256,
        provenanceSha256: $provenance_sha256,
        thirdPartyNoticesSha256: $third_party_notices_sha256},
      signer: {identity: $signing_identity, teamIdentifier: $team_identifier,
        leafCertificateSha256: $leaf_certificate_sha256},
      app: {bundleIdentifier: "com.thesongzhu.OpenOpen",
        manifestFile: "Contents/Resources/app-manifest.tsv",
        manifestFormat: "OPENOPEN-TREE-MANIFEST-V1",
        manifestSha256: $manifest_sha256,
        directoryCount: $directory_count, fileCount: $file_count,
        metadataPolicy: {types: ["directory", "regular-file"],
          directoryMode: "755", regularFileMode: "644", machOMode: "755",
          bsdFlags: "0", aclEntries: 0,
          allowedExtendedAttributes: ["com.apple.provenance"]},
        components: $components}}' >"$identity_json"

  /usr/bin/find -P "$receipt_staging" -type d -exec /bin/chmod 0755 {} +
  /usr/bin/find -P "$receipt_staging" -type f -exec /bin/chmod 0644 {} +
  /usr/bin/xattr -cr "$receipt_staging"
  /usr/bin/codesign --force --sign "$signing_identity" \
    --identifier com.thesongzhu.OpenOpen.PostStageIdentityReceipt \
    --options runtime --timestamp "$receipt_staging"
  /usr/bin/codesign --verify --strict "$receipt_staging"
  verify_signing_field "$receipt_staging" Identifier \
    com.thesongzhu.OpenOpen.PostStageIdentityReceipt
  verify_signing_field "$receipt_staging" TeamIdentifier "$imsg_team"
  verify_hardened_timestamped "$receipt_staging"
  verify_developer_id_application "$receipt_staging" "$imsg_team"
  verify_owner_certificate "$receipt_staging"
  verify_no_get_task_allow "$receipt_staging"
  [[ "$(entitlements_json "$receipt_staging")" == "{}" ]] || {
    echo "post-stage identity receipt has unexpected entitlements" >&2
    exit 66
  }
  /usr/bin/jq -e --arg manifest_sha "$app_manifest_sha" \
    '.schemaVersion == 2
      and .receiptKind == "com.thesongzhu.OpenOpen.post-stage-identity"
      and .app.manifestSha256 == $manifest_sha
      and (.app.components | length) == 4' "$identity_json" >/dev/null

  claimed_receipt=1
  /usr/bin/ditto "$receipt_staging" "$identity_receipt_output"
  /usr/bin/codesign --verify --strict "$identity_receipt_output"
  verify_signing_field "$identity_receipt_output" Identifier \
    com.thesongzhu.OpenOpen.PostStageIdentityReceipt
  verify_signing_field "$identity_receipt_output" TeamIdentifier "$imsg_team"
  verify_owner_certificate "$identity_receipt_output"
  source_receipt_manifest="$staging/source-receipt-manifest.tsv"
  output_receipt_manifest="$staging/output-receipt-manifest.tsv"
  write_tree_manifest "$receipt_staging" "$source_receipt_manifest"
  write_tree_manifest "$identity_receipt_output" "$output_receipt_manifest"
  /usr/bin/cmp -s "$source_receipt_manifest" "$output_receipt_manifest" || {
    echo "external post-stage identity receipt changed during publication" >&2
    exit 66
  }
  /bin/bash "$repo_root/scripts/create_alpha_dmg.sh" \
    --app "$output" \
    --identity-receipt "$identity_receipt_output" \
    --developer-id-identity "$signing_identity" \
    --verify-identity-receipt-only
  receipt_cdhash="$(signing_value "$identity_receipt_output" CDHash)"
  claimed_receipt=0
  claimed_output=0
  echo "STAGED_DEVELOPER_ID_WITH_EXTERNAL_IDENTITY_RECEIPT_NOT_NOTARIZED_NOT_RELEASE_PROOF $imsg_team $app_manifest_sha $receipt_cdhash $output $identity_receipt_output"
else
  claimed_output=0
  echo "STAGED_AD_HOC_NOT_RELEASE_PROOF $output"
fi
