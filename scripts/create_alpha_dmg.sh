#!/bin/bash
set -euo pipefail
umask 077

usage() {
  echo "usage: $0 --app ABSOLUTE_PATH --output ABSOLUTE_DMG [--developer-id-identity CERTIFICATE_NAME --identity-receipt ABSOLUTE_RECEIPT]" >&2
  echo "       $0 --app ABSOLUTE_PATH --identity-receipt ABSOLUTE_RECEIPT --developer-id-identity CERTIFICATE_NAME (--verify-identity-receipt-only|--test-identity-receipt)" >&2
  exit 64
}

app=""
output=""
identity_receipt=""
signing_identity=""
execution_mode="package"
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
    --identity-receipt)
      [[ $# -ge 2 ]] || usage
      identity_receipt="$2"
      shift 2
      ;;
    --developer-id-identity)
      [[ $# -ge 2 && -n "$2" ]] || usage
      signing_identity="$2"
      shift 2
      ;;
    --verify-identity-receipt-only)
      [[ "$execution_mode" == "package" ]] || usage
      execution_mode="verify"
      shift
      ;;
    --test-identity-receipt)
      [[ "$execution_mode" == "package" ]] || usage
      execution_mode="test"
      shift
      ;;
    *) usage ;;
  esac
done

[[ "$app" = /* ]] || usage
[[ -d "$app/Contents" ]] || {
  echo "missing staged OpenOpen app" >&2
  exit 65
}
if [[ "$execution_mode" == "package" ]]; then
  [[ "$output" = /* && "$output" == *.dmg ]] || usage
  [[ ! -e "$output" ]] || {
    echo "refusing to overwrite existing DMG: $output" >&2
    exit 65
  }
else
  [[ -z "$output" ]] || usage
fi
if [[ -n "$signing_identity" ]]; then
  [[ "$identity_receipt" = /* && "$identity_receipt" == *.receipt \
    && -d "$identity_receipt/Contents" ]] || {
    echo "Developer-ID verification requires an external signed .receipt bundle" >&2
    exit 65
  }
elif [[ -n "$identity_receipt" || "$execution_mode" != "package" ]]; then
  echo "identity-receipt verification requires Developer-ID mode" >&2
  exit 65
fi

repo_root="$(cd "$(dirname "$0")/.." && pwd -P)"

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

snapshot_external_pair() {
  local source_app="$1"
  local source_receipt="$2"
  local snapshot_root="$3"
  local app_before app_after app_private receipt_before receipt_after receipt_private
  app_before="$snapshot_root/source-app-before.tsv"
  app_after="$snapshot_root/source-app-after.tsv"
  app_private="$snapshot_root/private-app.tsv"
  receipt_before="$snapshot_root/source-receipt-before.tsv"
  receipt_after="$snapshot_root/source-receipt-after.tsv"
  receipt_private="$snapshot_root/private-receipt.tsv"
  write_tree_manifest "$source_app" "$app_before"
  write_tree_manifest "$source_receipt" "$receipt_before"
  /usr/bin/ditto "$source_app" "$snapshot_root/OpenOpen.app"
  /usr/bin/ditto "$source_receipt" \
    "$snapshot_root/OpenOpen-PostStage-Identity.receipt"
  write_tree_manifest "$source_app" "$app_after"
  write_tree_manifest "$source_receipt" "$receipt_after"
  write_tree_manifest "$snapshot_root/OpenOpen.app" "$app_private"
  write_tree_manifest "$snapshot_root/OpenOpen-PostStage-Identity.receipt" \
    "$receipt_private"
  if ! /usr/bin/cmp -s "$app_before" "$app_after" \
    || ! /usr/bin/cmp -s "$app_before" "$app_private" \
    || ! /usr/bin/cmp -s "$receipt_before" "$receipt_after" \
    || ! /usr/bin/cmp -s "$receipt_before" "$receipt_private"; then
    echo "App or identity receipt changed while taking its private snapshot" >&2
    return 66
  fi
}

link_file_exclusively() {
  # `link`, unlike the multi-form `ln` frontend, always treats its second
  # operand as the exact destination path and never redirects publication
  # into a directory that races into existence.
  /bin/link "$1" "$2"
}

publish_verified_file() {
  local staged_file="$1"
  local final_file="$2"
  local expected_sha="$3"
  local claim_publication="${4:-no-claim}"
  local staged_identity final_identity
  [[ "$claim_publication" == "claim-output" \
    || "$claim_publication" == "no-claim" ]] || {
    echo "verified artifact publication claim mode is invalid" >&2
    return 66
  }
  [[ -f "$staged_file" && ! -L "$staged_file" && ! -e "$final_file" \
    && "$(/usr/bin/shasum -a 256 "$staged_file" | /usr/bin/awk '{print $1}')" \
      == "$expected_sha" ]] || {
    echo "verified artifact publication precondition failed" >&2
    return 66
  }
  staged_identity="$(/usr/bin/stat -f '%d:%i' "$staged_file")"
  # The staging directory is created beside the destination, so an ordinary
  # hard link is a same-volume, kernel-enforced exclusive publication claim.
  # Unlike `mv -n`, link(2) cannot overwrite a destination that appears after
  # the precondition check: exactly one concurrent publisher can create the
  # final name. The staging name is removed only after the final name is bound
  # to the exact verified inode.
  link_file_exclusively "$staged_file" "$final_file" || {
    echo "verified artifact exclusive publication failed" >&2
    return 66
  }
  final_identity="$(/usr/bin/stat -f '%d:%i' "$final_file" 2>/dev/null || true)"
  [[ -f "$final_file" && ! -L "$final_file" \
    && "$final_identity" == "$staged_identity" ]] || {
    echo "verified artifact atomic publication failed" >&2
    return 66
  }
  if [[ "$claim_publication" == "claim-output" ]]; then
    published_output_identity="$staged_identity"
    claimed_output=1
  fi
  [[ "$(/usr/bin/shasum -a 256 "$final_file" | /usr/bin/awk '{print $1}')" \
    == "$expected_sha" ]] || {
    echo "verified artifact changed immediately after publication" >&2
    return 66
  }
  /bin/rm -f "$staged_file"
  final_identity="$(/usr/bin/stat -f '%d:%i' "$final_file" 2>/dev/null || true)"
  [[ ! -e "$staged_file" && -f "$final_file" && ! -L "$final_file" \
    && "$final_identity" == "$staged_identity" \
    && "$(/usr/bin/shasum -a 256 "$final_file" | /usr/bin/awk '{print $1}')" \
      == "$expected_sha" ]] || {
    echo "verified artifact publication changed while retiring staging" >&2
    return 66
  }
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

receipt_component_value() {
  local receipt="$1"
  local path="$2"
  local field="$3"
  /usr/bin/jq -er --arg path "$path" --arg field "$field" \
    '.app.components[] | select(.path == $path) | .[$field]' \
    "$receipt/Contents/Resources/identity.json"
}

verify_post_stage_identity_receipt() {
  local receipt="$1"
  local candidate="$2"
  local app_team="$3"
  local identity_json manifest_file scratch_manifest
  local source_snapshot_file scratch_source_snapshot source_snapshot_sha
  local actual_dirs expected_dirs actual_files expected_files
  local head status_sha diff_sha stage_script_sha dmg_script_sha
  local provenance_sha third_party_notices_sha manifest_sha
  local directory_count file_count relative identifier unsigned_sha
  local component component_cdhash component_signed_sha

  [[ -d "$receipt/Contents" && ! -L "$receipt" \
    && -z "$(/usr/bin/find -P "$receipt" -type l -print -quit)" \
    && -z "$(/usr/bin/find -P "$receipt" ! -type d ! -type f -print -quit)" ]] || {
    echo "external identity receipt has an invalid bundle shape" >&2
    exit 66
  }
  expected_dirs="$(/usr/bin/printf '%s\n' \
    . Contents Contents/Resources Contents/_CodeSignature | LC_ALL=C sort)"
  actual_dirs="$(cd "$receipt" && /usr/bin/find -P . -type d -print \
    | /usr/bin/sed 's#^\./##; s#^$#.#' | LC_ALL=C sort)"
  [[ "$actual_dirs" == "$expected_dirs" ]] || {
    echo "external identity receipt directory allowlist mismatch" >&2
    exit 66
  }
  expected_files="$(/usr/bin/printf '%s\n' \
    Contents/Info.plist \
    Contents/Resources/app-manifest.tsv \
    Contents/Resources/identity.json \
    Contents/Resources/source-snapshot.tsv \
    Contents/_CodeSignature/CodeDirectory \
    Contents/_CodeSignature/CodeRequirements \
    Contents/_CodeSignature/CodeResources \
    Contents/_CodeSignature/CodeSignature | LC_ALL=C sort)"
  actual_files="$(cd "$receipt" && /usr/bin/find -P . -type f -print \
    | /usr/bin/sed 's#^\./##' | LC_ALL=C sort)"
  [[ "$actual_files" == "$expected_files" ]] || {
    echo "external identity receipt file allowlist mismatch" >&2
    exit 66
  }
  while IFS= read -r relative; do
    [[ "$(/usr/bin/stat -f '%Lp' "$receipt/$relative")" == "755" ]] || {
      echo "external identity receipt directory mode mismatch: $relative" >&2
      exit 66
    }
    verify_entry_operational_metadata "$receipt/$relative"
  done <<<"$expected_dirs"
  while IFS= read -r relative; do
    [[ "$(/usr/bin/stat -f '%Lp' "$receipt/$relative")" == "644" ]] || {
      echo "external identity receipt file mode mismatch: $relative" >&2
      exit 66
    }
    verify_entry_operational_metadata "$receipt/$relative"
  done <<<"$expected_files"

  /usr/bin/codesign --verify --strict "$receipt"
  verify_signing_value "$receipt" Identifier \
    com.thesongzhu.OpenOpen.PostStageIdentityReceipt
  verify_signing_value "$receipt" TeamIdentifier "$app_team"
  verify_owner_certificate "$receipt"
  verify_developer_id_code "$receipt" "$app_team"
  [[ "$(entitlements_json "$receipt")" == "{}" ]] || {
    echo "external identity receipt has unexpected entitlements" >&2
    exit 66
  }
  [[ "$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' \
    "$receipt/Contents/Info.plist")" \
    == "com.thesongzhu.OpenOpen.PostStageIdentityReceipt" \
    && "$(/usr/libexec/PlistBuddy -c 'Print :CFBundleName' \
      "$receipt/Contents/Info.plist")" == "OpenOpenPostStageIdentityReceipt" \
    && "$(/usr/libexec/PlistBuddy -c 'Print :CFBundlePackageType' \
      "$receipt/Contents/Info.plist")" == "BNDL" \
    && "$(/usr/libexec/PlistBuddy -c 'Print :CFBundleVersion' \
      "$receipt/Contents/Info.plist")" == "2" ]] || {
    echo "external identity receipt Info.plist mismatch" >&2
    exit 66
  }

  identity_json="$receipt/Contents/Resources/identity.json"
  manifest_file="$receipt/Contents/Resources/app-manifest.tsv"
  /usr/bin/jq -e \
    --arg identity "$expected_developer_id_identity" \
    --arg team "$app_team" \
    --arg leaf "$expected_developer_id_leaf_sha" \
    'keys == ["app", "receiptKind", "schemaVersion", "signer", "source"]
      and .schemaVersion == 2
      and .receiptKind == "com.thesongzhu.OpenOpen.post-stage-identity"
      and (.source | keys) == ["binaryDiffSha256", "dmgScriptSha256", "head",
        "indexState", "provenanceSha256", "sourceSnapshotFile",
        "sourceSnapshotFormat", "sourceSnapshotSha256", "stageScriptSha256",
        "statusSha256", "thirdPartyNoticesSha256"]
      and .source.indexState == "empty"
      and .source.sourceSnapshotFile == "Contents/Resources/source-snapshot.tsv"
      and .source.sourceSnapshotFormat == "OPENOPEN-SOURCE-SNAPSHOT-V2"
      and .signer == {identity: $identity, teamIdentifier: $team,
        leafCertificateSha256: $leaf}
      and (.app | keys) == ["bundleIdentifier", "components", "directoryCount",
        "fileCount", "manifestFile", "manifestFormat", "manifestSha256",
        "metadataPolicy"]
      and .app.bundleIdentifier == "com.thesongzhu.OpenOpen"
      and .app.manifestFile == "Contents/Resources/app-manifest.tsv"
      and .app.manifestFormat == "OPENOPEN-TREE-MANIFEST-V1"
      and .app.directoryCount == 18 and .app.fileCount == 617
      and .app.metadataPolicy == {types: ["directory", "regular-file"],
        directoryMode: "755", regularFileMode: "644", machOMode: "755",
        bsdFlags: "0", aclEntries: 0,
        allowedExtendedAttributes: ["com.apple.provenance"]}
      and (.app.components | length) == 4
      and [.app.components[].path] == ["Contents/MacOS/OpenOpen",
        "Contents/MacOS/OpenOpenCore", "Contents/MacOS/OpenOpenEffectBroker",
        "Contents/MacOS/OpenOpenEffectBrokerWorker"]
      and all(.app.components[];
        (keys == ["cdhash", "entitlements", "hardenedRuntime", "identifier", "path",
          "secureTimestamp", "signedSha256", "teamIdentifier", "unsignedSha256"])
        and .teamIdentifier == $team and .hardenedRuntime == true
        and .secureTimestamp == true and .entitlements == {}
        and (.cdhash | test("^[0-9a-f]{40}$"))
        and (.signedSha256 | test("^[0-9a-f]{64}$"))
        and (.unsignedSha256 | test("^[0-9a-f]{64}$")))' \
    "$identity_json" >/dev/null || {
    echo "external identity receipt schema mismatch" >&2
    exit 66
  }

  head="$(git -C "$repo_root" rev-parse HEAD)"
  status_sha="$(git -C "$repo_root" status --porcelain=v1 -uall \
    | /usr/bin/shasum -a 256 | /usr/bin/awk '{print $1}')"
  diff_sha="$(git -C "$repo_root" diff --binary --no-ext-diff \
    | /usr/bin/shasum -a 256 | /usr/bin/awk '{print $1}')"
  stage_script_sha="$(/usr/bin/shasum -a 256 "$repo_root/scripts/stage_openopen_app.sh" \
    | /usr/bin/awk '{print $1}')"
  dmg_script_sha="$(/usr/bin/shasum -a 256 "$repo_root/scripts/create_alpha_dmg.sh" \
    | /usr/bin/awk '{print $1}')"
  provenance_sha="$(/usr/bin/shasum -a 256 "$repo_root/PROVENANCE.md" \
    | /usr/bin/awk '{print $1}')"
  third_party_notices_sha="$(/usr/bin/shasum -a 256 \
    "$repo_root/THIRD_PARTY_NOTICES.md" | /usr/bin/awk '{print $1}')"
  source_snapshot_file="$receipt/Contents/Resources/source-snapshot.tsv"
  [[ "$(/usr/bin/head -n 1 "$source_snapshot_file")" \
    == "OPENOPEN-SOURCE-SNAPSHOT-V2" ]] || {
    echo "external identity receipt source snapshot format mismatch" >&2
    exit 66
  }
  source_snapshot_sha="$(/usr/bin/shasum -a 256 "$source_snapshot_file" \
    | /usr/bin/awk '{print $1}')"
  [[ "$source_snapshot_sha" \
    == "$(/usr/bin/jq -er '.source.sourceSnapshotSha256' "$identity_json")" ]] || {
    echo "external identity receipt source snapshot digest mismatch" >&2
    exit 66
  }
  scratch_source_snapshot="$(/usr/bin/mktemp \
    /private/tmp/OpenOpen-source-snapshot.XXXXXX)"
  write_source_snapshot_manifest "$repo_root" "$scratch_source_snapshot"
  /usr/bin/cmp -s "$source_snapshot_file" "$scratch_source_snapshot" || {
    rm -f "$scratch_source_snapshot"
    echo "external identity receipt source snapshot mismatch" >&2
    exit 66
  }
  rm -f "$scratch_source_snapshot"
  /usr/bin/jq -e \
    --arg head "$head" --arg status "$status_sha" --arg diff "$diff_sha" \
    --arg stage "$stage_script_sha" --arg dmg "$dmg_script_sha" \
    --arg snapshot "$source_snapshot_sha" \
    --arg provenance "$provenance_sha" --arg notices "$third_party_notices_sha" \
    '.source == {head: $head, statusSha256: $status,
      binaryDiffSha256: $diff, indexState: "empty",
      sourceSnapshotFile: "Contents/Resources/source-snapshot.tsv",
      sourceSnapshotFormat: "OPENOPEN-SOURCE-SNAPSHOT-V2",
      sourceSnapshotSha256: $snapshot,
      stageScriptSha256: $stage,
      dmgScriptSha256: $dmg, provenanceSha256: $provenance,
      thirdPartyNoticesSha256: $notices}' "$identity_json" >/dev/null || {
    echo "external identity receipt source fingerprint mismatch" >&2
    exit 66
  }

  manifest_sha="$(/usr/bin/shasum -a 256 "$manifest_file" \
    | /usr/bin/awk '{print $1}')"
  [[ "$manifest_sha" == "$(/usr/bin/jq -er '.app.manifestSha256' "$identity_json")" ]] || {
    echo "external identity receipt manifest digest mismatch" >&2
    exit 66
  }
  scratch_manifest="$(/usr/bin/mktemp /private/tmp/OpenOpen-app-manifest.XXXXXX)"
  write_tree_manifest "$candidate" "$scratch_manifest"
  /usr/bin/cmp -s "$manifest_file" "$scratch_manifest" || {
    rm -f "$scratch_manifest"
    echo "App does not match its external post-stage identity receipt" >&2
    exit 66
  }
  rm -f "$scratch_manifest"
  directory_count="$(/usr/bin/find -P "$candidate" -type d -print \
    | /usr/bin/wc -l | /usr/bin/tr -d ' ')"
  file_count="$(/usr/bin/find -P "$candidate" -type f -print \
    | /usr/bin/wc -l | /usr/bin/tr -d ' ')"
  [[ "$directory_count" == "18" && "$file_count" == "617" ]] || {
    echo "App shape does not match its external identity receipt" >&2
    exit 66
  }

  while IFS='|' read -r relative identifier unsigned_sha; do
    component="$candidate/$relative"
    component_cdhash="$(signing_value "$component" CDHash)"
    component_signed_sha="$(/usr/bin/shasum -a 256 "$component" \
      | /usr/bin/awk '{print $1}')"
    /usr/bin/jq -e \
      --arg path "$relative" --arg identifier "$identifier" \
      --arg team "$app_team" --arg cdhash "$component_cdhash" \
      --arg signed "$component_signed_sha" --arg unsigned "$unsigned_sha" \
      '.app.components[] | select(.path == $path)
        | .identifier == $identifier and .teamIdentifier == $team
          and .cdhash == $cdhash and .signedSha256 == $signed
          and .unsignedSha256 == $unsigned and .hardenedRuntime == true
          and .secureTimestamp == true and .entitlements == {}' \
      "$identity_json" >/dev/null || {
      echo "component does not match external identity receipt: $relative" >&2
      exit 66
    }
  done <<EOF
Contents/MacOS/OpenOpen|com.thesongzhu.OpenOpen|9d978c5ac0f7cb83502ed9e041276fafbb1ec5afb6d32e3ba4520f562d97d884
Contents/MacOS/OpenOpenCore|com.thesongzhu.OpenOpen.Core|18ac46aab3de88730e95522f0a9b4c3ee6f4032a9d0ca6ca4e439df85b507708
Contents/MacOS/OpenOpenEffectBroker|com.thesongzhu.OpenOpen.EffectBroker|3ae8c92d4b50b6c0fc80c04d024b9d2c28279aa0fdf165294aac06563b595c78
Contents/MacOS/OpenOpenEffectBrokerWorker|com.thesongzhu.OpenOpen.EffectBroker.Worker|f78638f7716f9ab15fa3b9b1ba1951ef28e6d1f65f52e7a24a23bcae07cb0aab
EOF
}

verify_exact_developer_app() {
  local candidate="$1"
  local app_team="$2"
  local receipt="$3"
  local openai_team="2DC432GLL2"
  local rel path actual_macho expected_macho entitlement_json mode expected_mode
  local actual_app_files expected_app_files actual_app_dirs expected_app_dirs
  local notice_hashes runtime_sha app_cdhash core_cdhash broker_cdhash worker_cdhash
  local provenance_sha third_party_notices_sha

  verify_post_stage_identity_receipt "$receipt" "$candidate" "$app_team"
  app_cdhash="$(receipt_component_value "$receipt" \
    Contents/MacOS/OpenOpen cdhash)"
  core_cdhash="$(receipt_component_value "$receipt" \
    Contents/MacOS/OpenOpenCore cdhash)"
  broker_cdhash="$(receipt_component_value "$receipt" \
    Contents/MacOS/OpenOpenEffectBroker cdhash)"
  worker_cdhash="$(receipt_component_value "$receipt" \
    Contents/MacOS/OpenOpenEffectBrokerWorker cdhash)"
  provenance_sha="$(/usr/bin/jq -er '.source.provenanceSha256' \
    "$receipt/Contents/Resources/identity.json")"
  third_party_notices_sha="$(/usr/bin/jq -er '.source.thirdPartyNoticesSha256' \
    "$receipt/Contents/Resources/identity.json")"

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
Contents/MacOS/OpenOpen|com.thesongzhu.OpenOpen|$app_team|$app_cdhash|9d978c5ac0f7cb83502ed9e041276fafbb1ec5afb6d32e3ba4520f562d97d884
Contents/MacOS/OpenOpenCore|com.thesongzhu.OpenOpen.Core|$app_team|$core_cdhash|18ac46aab3de88730e95522f0a9b4c3ee6f4032a9d0ca6ca4e439df85b507708
Contents/MacOS/OpenOpenEffectBroker|com.thesongzhu.OpenOpen.EffectBroker|$app_team|$broker_cdhash|3ae8c92d4b50b6c0fc80c04d024b9d2c28279aa0fdf165294aac06563b595c78
Contents/MacOS/OpenOpenEffectBrokerWorker|com.thesongzhu.OpenOpen.EffectBroker.Worker|$app_team|$worker_cdhash|f78638f7716f9ab15fa3b9b1ba1951ef28e6d1f65f52e7a24a23bcae07cb0aab
Contents/Resources/Codex/0.144.0/bin/codex|codex|$openai_team|cf4f00c153b0ef5af3f71281d1a6c47be9c85c8e|-
Contents/Resources/Codex/0.144.0/bin/codex-code-mode-host|codex-code-mode-host|$openai_team|3ed966beb3746263b5d22e6ba0e81f41ace50f03|-
Contents/Resources/Codex/0.144.0/codex-path/rg|rg|$app_team|b117313f07e30d05462b942c318b1ae0b73b4e5c|ea91b02e833a93bea206911bb80434a837d11a4d2eca520548abd07cece2c2c6
Contents/Resources/iMessage/0.13.0/bin/imsg|com.thesongzhu.OpenOpen.imsg|$app_team|19de2b3e834adf95fed67c0cfd1a6f6a7759d5de|35ea30bce9b5c75403ba4dd68541a51916f41f5c6ba9df3a46882a4287556a6a
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

  verify_sha dc38dd78a8c3bfef736333257335667b8b87e28e205256637b751ac064f65ff7 \
    "$candidate/Contents/Resources/iMessage/0.13.0/BUILD-RECEIPT.json"
  verify_sha 818495226dda3332f711fc6d6408eacf1776e08fcddfa06342ab3f5196417839 \
    "$candidate/Contents/Resources/Notices/third_party/manifest.json"
  verify_sha "$provenance_sha" \
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
  verify_sha "$third_party_notices_sha" \
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
     and .buildReceiptSha256 == "dc38dd78a8c3bfef736333257335667b8b87e28e205256637b751ac064f65ff7"
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

test_post_stage_identity_receipt() (
  local candidate="$1"
  local receipt="$2"
  local app_team="$3"
  local scratch tampered wrong replay legacy bad_format drift
  local temporary_json source_fixture control_path publication_root
  local publication_sha
  local snapshot_one snapshot_two snapshot_sha
  scratch="$(/usr/bin/mktemp -d /private/tmp/OpenOpen-identity-receipt-test.XXXXXX)"
  trap 'rm -rf "$scratch"' EXIT
  tampered="$scratch/tampered.receipt"
  wrong="$scratch/wrong.receipt"
  replay="$scratch/replay.app"

  verify_exact_developer_app "$candidate" "$app_team" "$receipt"

  /usr/bin/ditto "$receipt" "$tampered"
  temporary_json="$scratch/tampered-identity.json"
  /usr/bin/jq '.app.manifestSha256 = ("0" * 64)' \
    "$tampered/Contents/Resources/identity.json" >"$temporary_json"
  /bin/chmod 0644 "$temporary_json"
  /bin/mv "$temporary_json" "$tampered/Contents/Resources/identity.json"
  if (verify_exact_developer_app "$candidate" "$app_team" "$tampered") \
    >/dev/null 2>&1; then
    echo "tampered external identity receipt was accepted" >&2
    exit 66
  fi
  echo "IDENTITY_RECEIPT_TAMPER_REJECTED=PASS"

  legacy="$scratch/legacy-v1.receipt"
  /usr/bin/ditto "$receipt" "$legacy"
  /usr/bin/plutil -replace CFBundleVersion -string 1 \
    "$legacy/Contents/Info.plist"
  temporary_json="$scratch/legacy-identity.json"
  /usr/bin/jq '.schemaVersion = 1' \
    "$legacy/Contents/Resources/identity.json" >"$temporary_json"
  /bin/chmod 0644 "$temporary_json"
  /bin/mv "$temporary_json" "$legacy/Contents/Resources/identity.json"
  /usr/bin/codesign --force --sign "$signing_identity" \
    --identifier com.thesongzhu.OpenOpen.PostStageIdentityReceipt \
    --options runtime --timestamp "$legacy"
  if (verify_exact_developer_app "$candidate" "$app_team" "$legacy") \
    >/dev/null 2>&1; then
    echo "historical identity receipt schema v1 was accepted" >&2
    exit 66
  fi
  echo "IDENTITY_RECEIPT_HISTORICAL_V1_REJECTED=PASS"

  bad_format="$scratch/bad-format.receipt"
  /usr/bin/ditto "$receipt" "$bad_format"
  /usr/bin/sed -i '' '1s/OPENOPEN-SOURCE-SNAPSHOT-V2/OPENOPEN-SOURCE-SNAPSHOT-V1/' \
    "$bad_format/Contents/Resources/source-snapshot.tsv"
  snapshot_sha="$(/usr/bin/shasum -a 256 \
    "$bad_format/Contents/Resources/source-snapshot.tsv" \
    | /usr/bin/awk '{print $1}')"
  temporary_json="$scratch/bad-format-identity.json"
  /usr/bin/jq --arg snapshot "$snapshot_sha" \
    '.source.sourceSnapshotSha256 = $snapshot' \
    "$bad_format/Contents/Resources/identity.json" >"$temporary_json"
  /bin/chmod 0644 "$temporary_json"
  /bin/mv "$temporary_json" "$bad_format/Contents/Resources/identity.json"
  /usr/bin/codesign --force --sign "$signing_identity" \
    --identifier com.thesongzhu.OpenOpen.PostStageIdentityReceipt \
    --options runtime --timestamp "$bad_format"
  if (verify_exact_developer_app "$candidate" "$app_team" "$bad_format") \
    >/dev/null 2>&1; then
    echo "source snapshot header/schema disagreement was accepted" >&2
    exit 66
  fi
  echo "IDENTITY_RECEIPT_SNAPSHOT_FORMAT_MISMATCH_REJECTED=PASS"

  /usr/bin/ditto "$receipt" "$wrong"
  /usr/bin/printf 'UNTRACKED\t644\t1\t%s\t%s\n' \
    "$(/usr/bin/printf x | /usr/bin/shasum -a 256 | /usr/bin/awk '{print $1}')" \
    synthetic/wrong-source.swift \
    >>"$wrong/Contents/Resources/source-snapshot.tsv"
  snapshot_sha="$(/usr/bin/shasum -a 256 \
    "$wrong/Contents/Resources/source-snapshot.tsv" | /usr/bin/awk '{print $1}')"
  temporary_json="$scratch/wrong-identity.json"
  /usr/bin/jq --arg snapshot "$snapshot_sha" \
    '.source.sourceSnapshotSha256 = $snapshot' \
    "$wrong/Contents/Resources/identity.json" >"$temporary_json"
  /bin/chmod 0644 "$temporary_json"
  /bin/mv "$temporary_json" "$wrong/Contents/Resources/identity.json"
  /usr/bin/codesign --force --sign "$signing_identity" \
    --identifier com.thesongzhu.OpenOpen.PostStageIdentityReceipt \
    --options runtime --timestamp "$wrong"
  /usr/bin/codesign --verify --strict "$wrong"
  if (verify_exact_developer_app "$candidate" "$app_team" "$wrong") \
    >/dev/null 2>&1; then
    echo "validly signed wrong-source identity receipt was accepted" >&2
    exit 66
  fi
  echo "IDENTITY_RECEIPT_WRONG_SNAPSHOT_REJECTED=PASS"

  /usr/bin/ditto "$candidate" "$replay"
  /usr/bin/codesign --force --sign "$signing_identity" \
    --identifier com.thesongzhu.OpenOpen --options runtime --timestamp "$replay"
  /usr/bin/codesign --verify --deep --strict "$replay"
  if (verify_exact_developer_app "$replay" "$app_team" "$receipt") \
    >/dev/null 2>&1; then
    echo "identity receipt replay against changed App metadata was accepted" >&2
    exit 66
  fi
  echo "IDENTITY_RECEIPT_REPLAY_REJECTED=PASS"

  drift="$scratch/post-publication-drift.app"
  /usr/bin/ditto "$candidate" "$drift"
  /bin/chmod 0600 "$drift/Contents/Info.plist"
  if (verify_exact_developer_app "$drift" "$app_team" "$receipt") \
    >/dev/null 2>&1; then
    echo "post-publication App metadata drift was accepted" >&2
    exit 66
  fi
  echo "IDENTITY_RECEIPT_POST_PUBLICATION_DRIFT_REJECTED=PASS"

  publication_root="$scratch/publication"
  mkdir "$publication_root"
  /usr/bin/printf 'verified\n' >"$publication_root/staged"
  publication_sha="$(/usr/bin/shasum -a 256 "$publication_root/staged" \
    | /usr/bin/awk '{print $1}')"
  /usr/bin/printf 'existing\n' >"$publication_root/final"
  if (publish_verified_file "$publication_root/staged" \
    "$publication_root/final" "$publication_sha") >/dev/null 2>&1; then
    echo "verified artifact publication overwrote an existing destination" >&2
    exit 66
  fi
  [[ -f "$publication_root/staged" \
    && "$(/bin/cat "$publication_root/final")" == "existing" ]] || {
    echo "failed artifact publication did not preserve both inputs" >&2
    exit 66
  }
  /usr/bin/printf 'directory-race\n' >"$publication_root/staged-directory-race"
  mkdir "$publication_root/final-directory-race"
  if link_file_exclusively "$publication_root/staged-directory-race" \
    "$publication_root/final-directory-race" >/dev/null 2>&1; then
    echo "exclusive publication treated a destination directory as a target" >&2
    exit 66
  fi
  [[ -f "$publication_root/staged-directory-race" \
    && -z "$(/usr/bin/find "$publication_root/final-directory-race" \
      -mindepth 1 -maxdepth 1 -print -quit)" ]] || {
    echo "destination-directory race left a nested publication artifact" >&2
    exit 66
  }
  /bin/ln -s "$publication_root/final-directory-race" \
    "$publication_root/final-symlink-directory-race"
  if link_file_exclusively "$publication_root/staged-directory-race" \
    "$publication_root/final-symlink-directory-race" >/dev/null 2>&1; then
    echo "exclusive publication followed a destination directory symlink" >&2
    exit 66
  fi
  [[ -f "$publication_root/staged-directory-race" \
    && -z "$(/usr/bin/find "$publication_root/final-directory-race" \
      -mindepth 1 -maxdepth 1 -print -quit)" ]] || {
    echo "destination-symlink race left a nested publication artifact" >&2
    exit 66
  }
  echo "VERIFIED_ARTIFACT_DIRECTORY_REDIRECTION_REJECTED=PASS"
  /usr/bin/printf 'verified\n' >"$publication_root/staged-success"
  claimed_output=0
  published_output_identity=""
  publish_verified_file "$publication_root/staged-success" \
    "$publication_root/final-success" "$publication_sha" claim-output
  [[ "$claimed_output" -eq 1 \
    && "$(/usr/bin/stat -f '%d:%i' "$publication_root/final-success")" \
      == "$published_output_identity" ]] || {
    echo "successful artifact publication was not immediately identity-owned" >&2
    exit 66
  }
  claimed_output=0
  echo "VERIFIED_ARTIFACT_ATOMIC_PUBLICATION=PASS"

  /usr/bin/printf 'publisher-one\n' >"$publication_root/staged-one"
  /usr/bin/printf 'publisher-two\n' >"$publication_root/staged-two"
  local publication_sha_one publication_sha_two publisher_one_pid publisher_two_pid
  local publisher_one_status publisher_two_status concurrent_value
  publication_sha_one="$(/usr/bin/shasum -a 256 \
    "$publication_root/staged-one" | /usr/bin/awk '{print $1}')"
  publication_sha_two="$(/usr/bin/shasum -a 256 \
    "$publication_root/staged-two" | /usr/bin/awk '{print $1}')"
  (publish_verified_file "$publication_root/staged-one" \
    "$publication_root/final-concurrent" "$publication_sha_one") \
    >"$publication_root/publisher-one.log" 2>&1 &
  publisher_one_pid=$!
  (publish_verified_file "$publication_root/staged-two" \
    "$publication_root/final-concurrent" "$publication_sha_two") \
    >"$publication_root/publisher-two.log" 2>&1 &
  publisher_two_pid=$!
  set +e
  wait "$publisher_one_pid"
  publisher_one_status=$?
  wait "$publisher_two_pid"
  publisher_two_status=$?
  set -e
  concurrent_value="$(/bin/cat "$publication_root/final-concurrent")"
  if [[ "$publisher_one_status" -eq 0 && "$publisher_two_status" -ne 0 ]]; then
    [[ "$concurrent_value" == "publisher-one" \
      && ! -e "$publication_root/staged-one" \
      && -f "$publication_root/staged-two" ]] || {
      echo "exclusive publisher one did not preserve the losing input" >&2
      exit 66
    }
  elif [[ "$publisher_two_status" -eq 0 && "$publisher_one_status" -ne 0 ]]; then
    [[ "$concurrent_value" == "publisher-two" \
      && ! -e "$publication_root/staged-two" \
      && -f "$publication_root/staged-one" ]] || {
      echo "exclusive publisher two did not preserve the losing input" >&2
      exit 66
    }
  else
    echo "exclusive concurrent publication did not produce exactly one winner" >&2
    exit 66
  fi
  echo "VERIFIED_ARTIFACT_CONCURRENT_EXCLUSIVE_PUBLICATION=PASS"

  source_fixture="$scratch/source-fixture"
  mkdir "$source_fixture"
  git -C "$source_fixture" init -q
  /usr/bin/printf 'tracked\n' >"$source_fixture/tracked.txt"
  git -C "$source_fixture" add tracked.txt
  git -C "$source_fixture" -c user.name=OpenOpen \
    -c user.email=openopen@example.invalid commit -q -m base
  /bin/chmod 0444 "$source_fixture/tracked.txt"
  write_source_snapshot_manifest "$source_fixture" \
    "$scratch/read-only-tracked.tsv"
  /usr/bin/grep -F $'TRACKED\t100644\t' \
    "$scratch/read-only-tracked.tsv" | /usr/bin/grep -F $'\t444\t' >/dev/null || {
    echo "read-only tracked source mode was not accepted and bound" >&2
    exit 66
  }
  /bin/chmod 0644 "$source_fixture/tracked.txt"
  echo "SOURCE_SNAPSHOT_NONGIT_MODE_BOUND=PASS"
  /usr/bin/printf 'one\n' >"$source_fixture/untracked.txt"
  snapshot_one="$scratch/source-snapshot-one.tsv"
  snapshot_two="$scratch/source-snapshot-two.tsv"
  write_source_snapshot_manifest "$source_fixture" "$snapshot_one"
  /usr/bin/printf 'two\n' >"$source_fixture/untracked.txt"
  write_source_snapshot_manifest "$source_fixture" "$snapshot_two"
  [[ "$(/usr/bin/shasum -a 256 "$snapshot_one" | /usr/bin/awk '{print $1}')" \
    != "$(/usr/bin/shasum -a 256 "$snapshot_two" | /usr/bin/awk '{print $1}')" ]] || {
    echo "untracked source byte drift did not change the source snapshot" >&2
    exit 66
  }
  echo "SOURCE_SNAPSHOT_UNTRACKED_BYTES_BOUND=PASS"
  /usr/bin/printf 'staged\n' >"$source_fixture/tracked.txt"
  git -C "$source_fixture" add tracked.txt
  if (write_source_snapshot_manifest "$source_fixture" \
    "$scratch/staged-source-snapshot.tsv") >/dev/null 2>&1; then
    echo "staged source content was accepted by an empty-index receipt" >&2
    exit 66
  fi
  echo "SOURCE_SNAPSHOT_STAGED_CONTENT_REJECTED=PASS"

  git -C "$source_fixture" reset --hard -q HEAD
  rm -f "$source_fixture/untracked.txt"
  write_source_snapshot_manifest "$source_fixture" "$snapshot_one"
  /usr/bin/printf 'worktree-drift\n' >"$source_fixture/tracked.txt"
  write_source_snapshot_manifest "$source_fixture" "$snapshot_two"
  [[ "$(/usr/bin/shasum -a 256 "$snapshot_one" | /usr/bin/awk '{print $1}')" \
    != "$(/usr/bin/shasum -a 256 "$snapshot_two" | /usr/bin/awk '{print $1}')" ]] || {
    echo "tracked worktree byte drift did not change the source snapshot" >&2
    exit 66
  }
  echo "SOURCE_SNAPSHOT_TRACKED_BYTES_BOUND=PASS"

  git -C "$source_fixture" reset --hard -q HEAD
  git -C "$source_fixture" update-index --assume-unchanged tracked.txt
  if (write_source_snapshot_manifest "$source_fixture" \
    "$scratch/assume-unchanged.tsv") >/dev/null 2>&1; then
    echo "assume-unchanged index state was accepted" >&2
    exit 66
  fi
  git -C "$source_fixture" update-index --no-assume-unchanged tracked.txt
  echo "SOURCE_SNAPSHOT_ASSUME_UNCHANGED_REJECTED=PASS"

  git -C "$source_fixture" update-index --skip-worktree tracked.txt
  if (write_source_snapshot_manifest "$source_fixture" \
    "$scratch/skip-worktree.tsv") >/dev/null 2>&1; then
    echo "skip-worktree index state was accepted" >&2
    exit 66
  fi
  git -C "$source_fixture" update-index --no-skip-worktree tracked.txt
  echo "SOURCE_SNAPSHOT_SKIP_WORKTREE_REJECTED=PASS"

  /usr/bin/printf 'intent\n' >"$source_fixture/intent.txt"
  git -C "$source_fixture" add -N intent.txt
  if (write_source_snapshot_manifest "$source_fixture" \
    "$scratch/intent-to-add.tsv") >/dev/null 2>&1; then
    echo "intent-to-add index state was accepted" >&2
    exit 66
  fi
  git -C "$source_fixture" reset -q
  rm -f "$source_fixture/intent.txt"
  echo "SOURCE_SNAPSHOT_INTENT_TO_ADD_REJECTED=PASS"

  control_path="$source_fixture/"$'control\rname.txt'
  /usr/bin/printf 'control\n' >"$control_path"
  if (write_source_snapshot_manifest "$source_fixture" \
    "$scratch/control-path.tsv") >/dev/null 2>&1; then
    echo "control-byte source path was accepted" >&2
    exit 66
  fi
  rm -f "$control_path"
  echo "SOURCE_SNAPSHOT_CONTROL_PATH_REJECTED=PASS"

  echo "IDENTITY_RECEIPT_FOCUSED_TESTS=PASS"
)

original_app="$app"
original_identity_receipt="$identity_receipt"
input_snapshot=""
cleanup_input_snapshot() {
  if [[ -n "$input_snapshot" ]]; then
    rm -rf "$input_snapshot"
  fi
}
trap cleanup_input_snapshot EXIT
if [[ -n "$signing_identity" ]]; then
  input_snapshot="$(/usr/bin/mktemp -d \
    /private/tmp/OpenOpen-verified-input.XXXXXX)"
  snapshot_external_pair "$app" "$identity_receipt" "$input_snapshot"
  app="$input_snapshot/OpenOpen.app"
  identity_receipt="$input_snapshot/OpenOpen-PostStage-Identity.receipt"
fi

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
  verify_exact_developer_app "$app" "$app_team" "$identity_receipt"
fi

if [[ "$execution_mode" == "verify" ]]; then
  echo "POST_STAGE_IDENTITY_RECEIPT_VERIFY=PASS $app_team $original_app $original_identity_receipt"
  exit 0
fi
if [[ "$execution_mode" == "test" ]]; then
  test_post_stage_identity_receipt "$app" "$identity_receipt" "$app_team"
  exit 0
fi

output_parent="$(dirname "$output")"
mkdir -p "$output_parent"
output_parent="$(cd "$output_parent" && pwd -P)"
output="$output_parent/$(basename "$output")"
staging="$(/usr/bin/mktemp -d "$output_parent/.OpenOpen-alpha-dmg.XXXXXX")"
mountpoint="$(/usr/bin/mktemp -d /private/tmp/OpenOpen-alpha-mount.XXXXXX)"
mounted=0
claimed_output=0
published_output_identity=""
cleanup() {
  local current_output_identity
  if [[ "$mounted" -eq 1 ]]; then
    /usr/bin/hdiutil detach "$mountpoint" -quiet || true
  fi
  rm -rf "$mountpoint" "$staging" "$input_snapshot"
  if [[ "$claimed_output" -eq 1 ]]; then
    current_output_identity="$(/usr/bin/stat -f '%d:%i' "$output" \
      2>/dev/null || true)"
    if [[ -n "$published_output_identity" \
      && "$current_output_identity" == "$published_output_identity" ]]; then
      rm -f "$output"
    else
      echo "refusing to remove a drifted or non-owned final DMG path" >&2
    fi
  fi
}
trap cleanup EXIT

disk_root="$staging/disk"
install_root="$staging/install-test"
dmg_staging="$staging/OpenOpen-alpha.dmg"
mkdir "$disk_root" "$install_root"
/usr/bin/ditto "$app" "$disk_root/OpenOpen.app"
ln -s /Applications "$disk_root/Applications"
if [[ -n "$signing_identity" ]]; then
  /usr/bin/ditto "$identity_receipt" \
    "$disk_root/.OpenOpen-PostStage-Identity.receipt"
  verify_exact_developer_app "$disk_root/OpenOpen.app" "$app_team" \
    "$disk_root/.OpenOpen-PostStage-Identity.receipt"
  source_receipt_tree_manifest="$staging/source-receipt-tree-manifest.tsv"
  disk_receipt_tree_manifest="$staging/disk-receipt-tree-manifest.tsv"
  write_tree_manifest "$identity_receipt" "$source_receipt_tree_manifest"
  write_tree_manifest "$disk_root/.OpenOpen-PostStage-Identity.receipt" \
    "$disk_receipt_tree_manifest"
  /usr/bin/cmp -s "$source_receipt_tree_manifest" \
    "$disk_receipt_tree_manifest" || {
    echo "external identity receipt changed before DMG creation" >&2
    exit 66
  }
fi
/usr/bin/hdiutil create \
  -quiet \
  -volname "OpenOpen Alpha" \
  -srcfolder "$disk_root" \
  -format UDZO \
  "$dmg_staging"
if [[ -n "$signing_identity" ]]; then
  /usr/bin/codesign --force --sign "$signing_identity" --timestamp "$dmg_staging"
  /usr/bin/codesign --verify --strict "$dmg_staging"
  dmg_team="$(/usr/bin/codesign -d --verbose=4 "$dmg_staging" 2>&1 \
    | /usr/bin/awk -F= '$1 == "TeamIdentifier" {print $2}')"
  [[ "$dmg_team" == "$app_team" ]] || {
    echo "DMG and app TeamIdentifier values differ" >&2
    exit 66
  }
  /usr/bin/codesign --verify --strict --test-requirement "$app_requirement" \
    "$dmg_staging" || {
    echo "DMG is not signed by the app Team Developer ID Application" >&2
    exit 66
  }
  verify_owner_certificate "$dmg_staging"
  dmg_details="$(/usr/bin/codesign -d --verbose=4 "$dmg_staging" 2>&1)"
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
  "$dmg_staging"
mounted=1
[[ -d "$mountpoint/OpenOpen.app/Contents" ]] || {
  echo "mounted alpha DMG is missing OpenOpen.app" >&2
  exit 66
}
[[ -L "$mountpoint/Applications" \
  && "$(/usr/bin/readlink "$mountpoint/Applications")" == "/Applications" ]] || {
  echo "mounted alpha DMG root shape mismatch" >&2
  exit 66
}
mounted_root_entries="$(cd "$mountpoint" && /usr/bin/find -P . -mindepth 1 \
  -maxdepth 1 -print | /usr/bin/sed 's#^\./##' | LC_ALL=C sort)"
if [[ -n "$signing_identity" ]]; then
  [[ -d "$mountpoint/.OpenOpen-PostStage-Identity.receipt/Contents" ]] || {
    echo "mounted alpha DMG is missing its sealed external identity receipt" >&2
    exit 66
  }
  expected_root_entries="$(/usr/bin/printf '%s\n' \
    .OpenOpen-PostStage-Identity.receipt Applications OpenOpen.app | LC_ALL=C sort)"
else
  expected_root_entries="$(/usr/bin/printf '%s\n' Applications OpenOpen.app \
    | LC_ALL=C sort)"
fi
[[ "$mounted_root_entries" == "$expected_root_entries" ]] || {
  echo "mounted alpha DMG contains an unexpected root entry" >&2
  exit 66
}
/usr/bin/codesign --verify --deep --strict "$mountpoint/OpenOpen.app"
if [[ -n "$signing_identity" ]]; then
  verify_exact_developer_app "$mountpoint/OpenOpen.app" "$app_team" \
    "$mountpoint/.OpenOpen-PostStage-Identity.receipt"
  mounted_receipt_tree_manifest="$staging/mounted-receipt-tree-manifest.tsv"
  write_tree_manifest "$mountpoint/.OpenOpen-PostStage-Identity.receipt" \
    "$mounted_receipt_tree_manifest"
  /usr/bin/cmp -s "$source_receipt_tree_manifest" \
    "$mounted_receipt_tree_manifest" || {
    echo "mounted external identity receipt differs from its sealed source" >&2
    exit 66
  }
fi
/usr/bin/ditto "$mountpoint/OpenOpen.app" "$install_root/OpenOpen.app"
/usr/bin/codesign --verify --deep --strict "$install_root/OpenOpen.app"
if [[ -n "$signing_identity" ]]; then
  verify_exact_developer_app "$install_root/OpenOpen.app" "$app_team" \
    "$identity_receipt"
fi
/usr/bin/hdiutil detach "$mountpoint" -quiet
mounted=0

dmg_sha="$(/usr/bin/shasum -a 256 "$dmg_staging" | /usr/bin/awk '{print $1}')"
publish_verified_file "$dmg_staging" "$output" "$dmg_sha" claim-output
if [[ -n "$signing_identity" ]]; then
  /usr/bin/codesign --verify --strict "$output"
  verify_owner_certificate "$output"
fi
[[ "$(/usr/bin/stat -f '%d:%i' "$output" 2>/dev/null || true)" \
    == "$published_output_identity" \
  && "$(/usr/bin/shasum -a 256 "$output" | /usr/bin/awk '{print $1}')" \
    == "$dmg_sha" ]] || {
  echo "published DMG identity or digest changed at the final success boundary" >&2
  exit 66
}
claimed_output=0
if [[ -n "$signing_identity" ]]; then
  echo "ALPHA_DMG_DEVELOPER_ID_NOT_NOTARIZED_NOT_RELEASE_PROOF $app_team $dmg_sha $output"
else
  echo "ALPHA_DMG_NOT_NOTARIZED_NOT_RELEASE_PROOF $dmg_sha $output"
fi
