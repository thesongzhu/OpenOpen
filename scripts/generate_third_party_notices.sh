#!/usr/bin/env bash
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/.." && pwd)
DEST="$ROOT/third_party/notices"
DOC="$ROOT/THIRD_PARTY_NOTICES.md"
TARGET=aarch64-apple-darwin
CODEX_COMMIT=767822446c7a594caa19609ca435281a9ec67e0d
IMSG_COMMIT=fa2f82d7dbda4c802d91c1d41bb6c53564ed2fdc
FRIDAY_COMMIT=4870f31fa088bef7eb9f4f256ec62993b02eda80
RIPGREP_COMMIT=af60c2de9d85e7f3d81c78601669468cf02dabab
SERENITY_COMMIT=1809beb0fc24f3942c500058ad4fa47e6a97d3f9
CHECK=0
CODEX_SOURCE_ROOT=
IMSG_SOURCE_ROOT=
FRIDAY_SOURCE_ROOT=
RIPGREP_SOURCE_ROOT=

usage() {
  printf '%s\n' \
    "usage: $0 --codex-source-root PATH --imsg-source-root PATH" \
    "          --friday-source-root PATH --ripgrep-source-root PATH [--check]"
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --codex-source-root) CODEX_SOURCE_ROOT=${2:?missing path}; shift 2 ;;
    --imsg-source-root) IMSG_SOURCE_ROOT=${2:?missing path}; shift 2 ;;
    --friday-source-root) FRIDAY_SOURCE_ROOT=${2:?missing path}; shift 2 ;;
    --ripgrep-source-root) RIPGREP_SOURCE_ROOT=${2:?missing path}; shift 2 ;;
    --check) CHECK=1; shift ;;
    -h|--help) usage; exit 0 ;;
    *) usage >&2; exit 2 ;;
  esac
done

for command in cargo git jq shasum tar; do
  command -v "$command" >/dev/null || { printf 'missing command: %s\n' "$command" >&2; exit 1; }
done
for value in CODEX_SOURCE_ROOT IMSG_SOURCE_ROOT FRIDAY_SOURCE_ROOT RIPGREP_SOURCE_ROOT; do
  eval "path=\${$value}"
  [ -n "$path" ] && [ -d "$path" ] || { printf 'missing required source root: %s\n' "$value" >&2; exit 1; }
done

require_commit() {
  local root=$1 expected=$2 label=$3
  local actual
  actual=$(git -C "$root" rev-parse HEAD)
  [ "$actual" = "$expected" ] || {
    printf '%s source mismatch: expected %s, got %s\n' "$label" "$expected" "$actual" >&2
    exit 1
  }
}

require_commit "$CODEX_SOURCE_ROOT" "$CODEX_COMMIT" Codex
require_commit "$IMSG_SOURCE_ROOT" "$IMSG_COMMIT" imsg
require_commit "$RIPGREP_SOURCE_ROOT" "$RIPGREP_COMMIT" ripgrep
git -C "$FRIDAY_SOURCE_ROOT" cat-file -e "$FRIDAY_COMMIT^{commit}"

TMP=$(mktemp -d /private/tmp/openopen-notices.XXXXXX)
trap 'rm -rf "$TMP"' EXIT
WORK="$TMP/notices"
TEXTS="$WORK/texts"
mkdir -p "$TEXTS"

hash_file() {
  shasum -a 256 "$1" | awk '{print $1}'
}

store_text() {
  local file=$1 hash target
  [ -s "$file" ] || { printf 'empty notice text: %s\n' "$file" >&2; exit 1; }
  hash=$(hash_file "$file")
  target="$TEXTS/$hash.txt"
  if [ -e "$target" ]; then
    cmp -s "$file" "$target" || { printf 'SHA-256 collision: %s\n' "$hash" >&2; exit 1; }
  else
    cp "$file" "$target"
  fi
  printf '%s' "$hash"
}

extract_closure() {
  local metadata=$1 root_a=$2 root_b=$3 output=$4
  jq -r --arg root_a "$root_a" --arg root_b "$root_b" '
    . as $m
    | (reduce $m.resolve.nodes[] as $n ({};
        .[$n.id] = [$n.deps[] | select(any(.dep_kinds[]; .kind != "dev")) | .pkg])) as $graph
    | [$m.packages[] | select(.name == $root_a or .name == $root_b) | .id] as $roots
    | if ($roots | length) != 2 then error("expected exactly two closure roots") else . end
    | def expand($ids):
        (($ids + [$ids[] as $id | $graph[$id][]?]) | unique) as $next
        | if $next == $ids then $ids else expand($next) end;
      expand($roots) as $ids
    | $m.packages[]
    | select(.source != null and (.id as $id | $ids | index($id)))
    | [.name, .version, .source, .license, .manifest_path, (.license_file // "")]
    | @tsv
  ' "$metadata" | LC_ALL=C sort -t $'\t' -k1,1 -k2,2 -k3,3 > "$output"
  [ -s "$output" ] || { printf 'empty Rust dependency closure\n' >&2; exit 1; }
}

OPENOPEN_METADATA="$TMP/openopen-metadata.json"
cargo metadata --locked --offline --format-version 1 --filter-platform "$TARGET" \
  --manifest-path "$ROOT/Cargo.toml" > "$OPENOPEN_METADATA"
extract_closure "$OPENOPEN_METADATA" openopen-host openopen-effect-broker "$TMP/openopen.tsv"

# The official 0.144.0 release commit versions workspace manifests as 0.144.0,
# while its committed lock records those path-only packages as 0.0.0. Generate
# the graph in an archive and reject every lock change except that known local
# workspace version normalization. Registry/git identities and checksums remain
# byte-for-byte bound to the committed lock.
CODEX_ARCHIVE="$TMP/codex-source"
mkdir -p "$CODEX_ARCHIVE"
git -C "$CODEX_SOURCE_ROOT" archive "$CODEX_COMMIT" | tar -x -C "$CODEX_ARCHIVE"
cp "$CODEX_ARCHIVE/codex-rs/Cargo.lock" "$TMP/codex-Cargo.lock.before"
CODEX_METADATA="$TMP/codex-metadata.json"
cargo metadata --offline --format-version 1 --filter-platform "$TARGET" \
  --manifest-path "$CODEX_ARCHIVE/codex-rs/Cargo.toml" > "$CODEX_METADATA"
diff -u "$TMP/codex-Cargo.lock.before" "$CODEX_ARCHIVE/codex-rs/Cargo.lock" > "$TMP/codex-lock.diff" || true
awk 'BEGIN { RS="\\[\\[package\\]\\]" } /source = / { print "[[package]]" $0 }' \
  "$TMP/codex-Cargo.lock.before" > "$TMP/codex-third-party.before"
awk 'BEGIN { RS="\\[\\[package\\]\\]" } /source = / { print "[[package]]" $0 }' \
  "$CODEX_ARCHIVE/codex-rs/Cargo.lock" > "$TMP/codex-third-party.after"
cmp "$TMP/codex-third-party.before" "$TMP/codex-third-party.after" || {
  printf 'Codex third-party lock identities or checksums changed\n' >&2
  exit 1
}
awk '
  /^---/ || /^\+\+\+/ { next }
  /^[-+]version = / {
    if ($0 != "-version = \"0.0.0\"" && $0 != "+version = \"0.144.0\"") bad=1
    next
  }
  /^[-+]/ { bad=1 }
  END { exit bad }
' "$TMP/codex-lock.diff" || {
  printf 'Codex metadata changed more than path-only workspace versions\n' >&2
  exit 1
}
extract_closure "$CODEX_METADATA" codex-cli codex-code-mode-host "$TMP/codex.tsv"

emit_rust_closure() {
  local component=$1 package_tsv=$2 output=$3
  : > "$output"
  while IFS=$'\t' read -r name version source license manifest_path license_file; do
    [ -n "$name" ] && [ -n "$version" ] && [ -n "$source" ] && [ -n "$license" ] || {
      printf 'incomplete Rust package metadata: %s %s\n' "$name" "$version" >&2
      exit 1
    }
    local package_dir candidates docs file base lower rel hash identity
    package_dir=$(dirname "$manifest_path")
    candidates="$TMP/candidates.$$.txt"
    docs="$TMP/docs.$$.ndjson"
    : > "$candidates"
    : > "$docs"
    if [ -n "$license_file" ] && [ -f "$license_file" ]; then
      printf '%s\n' "$license_file" >> "$candidates"
    fi
    while IFS= read -r file; do
      base=$(basename "$file")
      lower=$(printf '%s' "$base" | tr '[:upper:]' '[:lower:]')
      case "$lower" in
        license*|licence*|copying*|notice*|unlicense*) printf '%s\n' "$file" >> "$candidates" ;;
      esac
    done < <(find "$package_dir" -maxdepth 1 -type f -print | LC_ALL=C sort)
    LC_ALL=C sort -u "$candidates" -o "$candidates"
    # Some crates intentionally omit a standalone license file from their
    # published archive. Preserve their exact packaged declaration (license,
    # authors, repository) instead of inventing an unpinned replacement text.
    if [ ! -s "$candidates" ]; then
      if [ -s "$package_dir/Cargo.toml.orig" ]; then
        printf '%s\n' "$package_dir/Cargo.toml.orig" > "$candidates"
      elif [ -s "$package_dir/Cargo.toml" ]; then
        printf '%s\n' "$package_dir/Cargo.toml" > "$candidates"
      fi
    fi
    [ -s "$candidates" ] || {
      printf 'no license/notice text for %s %s (%s)\n' "$name" "$version" "$source" >&2
      exit 1
    }
    while IFS= read -r file; do
      hash=$(store_text "$file")
      case "$file" in
        "$package_dir"/*) rel=${file#"$package_dir"/} ;;
        *) rel=$(basename "$file") ;;
      esac
      jq -cn --arg sourcePath "$rel" --arg textSha256 "$hash" \
        '{sourcePath:$sourcePath,textSha256:$textSha256}' >> "$docs"
    done < "$candidates"
    identity="$name@$version|$source"
    jq -cn \
      --arg identity "$identity" --arg name "$name" --arg version "$version" \
      --arg source "$source" --arg license "$license" --arg component "$component" \
      --slurpfile documents "$docs" \
      '{component:$component,identity:$identity,name:$name,version:$version,source:$source,license:$license,documents:$documents}' \
      >> "$output"
  done < "$package_tsv"
}

emit_rust_closure openopen "$TMP/openopen.tsv" "$TMP/openopen.ndjson"
emit_rust_closure codex "$TMP/codex.tsv" "$TMP/codex.ndjson"

COMPONENTS="$TMP/components.ndjson"
: > "$COMPONENTS"
emit_component() {
  local id=$1 version=$2 source=$3 license=$4 revision=$5 file=$6 source_path=$7
  local hash
  hash=$(store_text "$file")
  jq -cn \
    --arg id "$id" --arg version "$version" --arg source "$source" --arg license "$license" \
    --arg revision "$revision" --arg sourcePath "$source_path" --arg textSha256 "$hash" \
    '{id:$id,version:$version,source:$source,license:$license,revision:$revision,documents:[{sourcePath:$sourcePath,textSha256:$textSha256}]}' \
    >> "$COMPONENTS"
}

git -C "$FRIDAY_SOURCE_ROOT" show "$FRIDAY_COMMIT:LICENSE" > "$TMP/friday-LICENSE"
emit_component friday-source "$FRIDAY_COMMIT" https://github.com/thesongzhu/Friday MIT "$FRIDAY_COMMIT" "$TMP/friday-LICENSE" LICENSE
emit_component imsg 0.13.0 https://github.com/openclaw/imsg MIT "$IMSG_COMMIT" "$IMSG_SOURCE_ROOT/LICENSE" LICENSE

COMMANDER_ROOT="$IMSG_SOURCE_ROOT/.build/checkouts/Commander"
PHONE_ROOT="$IMSG_SOURCE_ROOT/.build/checkouts/PhoneNumberKit"
SQLITE_ROOT="$IMSG_SOURCE_ROOT/.build/checkouts/SQLite.swift"
SWIFT_LOCK="$ROOT/third_party/imsg/Package.resolved"
[ -s "$SWIFT_LOCK" ] || { printf 'missing tracked imsg Package.resolved\n' >&2; exit 1; }
jq -e '
  (.pins | length) == 3 and
  any(.pins[]; .identity=="commander" and .location=="https://github.com/steipete/Commander.git" and .state.version=="0.2.4" and .state.revision=="bd219c4ee9032fee3e009856f81fcc6ec09a85f4") and
  any(.pins[]; .identity=="phonenumberkit" and .location=="https://github.com/PhoneNumberKit/PhoneNumberKit.git" and .state.version=="5.0.4" and .state.revision=="ab06a8333394f4a4fb6eecca447dae0aa06c1eca") and
  any(.pins[]; .identity=="sqlite.swift" and .location=="https://github.com/stephencelis/SQLite.swift.git" and .state.version=="0.16.0" and .state.revision=="964c300fb0736699ce945c9edb56ecd62eba27a3")
' "$SWIFT_LOCK" >/dev/null || { printf 'tracked imsg Swift pins drifted\n' >&2; exit 1; }
require_commit "$COMMANDER_ROOT" bd219c4ee9032fee3e009856f81fcc6ec09a85f4 Commander
require_commit "$PHONE_ROOT" ab06a8333394f4a4fb6eecca447dae0aa06c1eca PhoneNumberKit
require_commit "$SQLITE_ROOT" 964c300fb0736699ce945c9edb56ecd62eba27a3 SQLite.swift
emit_component commander 0.2.4 https://github.com/steipete/Commander MIT bd219c4ee9032fee3e009856f81fcc6ec09a85f4 "$COMMANDER_ROOT/LICENSE" LICENSE
emit_component phone-number-kit 5.0.4 https://github.com/PhoneNumberKit/PhoneNumberKit.git MIT ab06a8333394f4a4fb6eecca447dae0aa06c1eca "$PHONE_ROOT/LICENSE" LICENSE
emit_component sqlite-swift 0.16.0 https://github.com/stephencelis/SQLite.swift MIT 964c300fb0736699ce945c9edb56ecd62eba27a3 "$SQLITE_ROOT/LICENSE.txt" LICENSE.txt
emit_component codex-root 0.144.0 https://github.com/openai/codex Apache-2.0 "$CODEX_COMMIT" "$CODEX_SOURCE_ROOT/LICENSE" LICENSE
emit_component codex-notice 0.144.0 https://github.com/openai/codex Apache-2.0 "$CODEX_COMMIT" "$CODEX_SOURCE_ROOT/NOTICE" NOTICE
emit_component ripgrep-copying 15.1.0 https://github.com/BurntSushi/ripgrep 'MIT OR Unlicense' "$RIPGREP_COMMIT" "$RIPGREP_SOURCE_ROOT/COPYING" COPYING
emit_component ripgrep-mit 15.1.0 https://github.com/BurntSushi/ripgrep MIT "$RIPGREP_COMMIT" "$RIPGREP_SOURCE_ROOT/LICENSE-MIT" LICENSE-MIT
emit_component ripgrep-unlicense 15.1.0 https://github.com/BurntSushi/ripgrep Unlicense "$RIPGREP_COMMIT" "$RIPGREP_SOURCE_ROOT/UNLICENSE" UNLICENSE

jq -s 'sort_by(.identity)' "$TMP/openopen.ndjson" > "$TMP/openopen.json"
jq -s 'sort_by(.identity)' "$TMP/codex.ndjson" > "$TMP/codex.json"
jq -s 'sort_by(.id)' "$COMPONENTS" > "$TMP/components.json"

jq -S -n \
  --arg target "$TARGET" \
  --arg codexCommit "$CODEX_COMMIT" --arg imsgCommit "$IMSG_COMMIT" \
  --arg fridayCommit "$FRIDAY_COMMIT" --arg ripgrepCommit "$RIPGREP_COMMIT" \
  --arg serenityCommit "$SERENITY_COMMIT" \
  --slurpfile openopen "$TMP/openopen.json" --slurpfile codex "$TMP/codex.json" \
  --slurpfile components "$TMP/components.json" \
  '{schemaVersion:1,target:$target,scope:{roots:{openopen:["openopen-host","openopen-effect-broker"],codex:["codex-cli","codex-code-mode-host"]},dependencyKinds:["normal","build"]},inputs:{codex:{version:"0.144.0",commit:$codexCommit},imsg:{version:"0.13.0",commit:$imsgCommit},friday:{commit:$fridayCommit},ripgrep:{version:"15.1.0",commit:$ripgrepCommit},serenity:{version:"0.12.5",commit:$serenityCommit}},closures:{openopenRust:$openopen[0],codexRust:$codex[0]},components:$components[0]}' \
  > "$WORK/manifest.json"

validate_manifest() {
  local manifest=$1
  jq -e --arg serenity "$SERENITY_COMMIT" '
    (.closures.openopenRust | length) > 0 and
    (.closures.codexRust | length) > 0 and
    ([.closures.openopenRust[],.closures.codexRust[]] | all(
      (.identity|length)>0 and (.source|length)>0 and (.license|length)>0 and
      (.documents|length)>0 and (.documents|all((.sourcePath|length)>0 and (.textSha256|test("^[0-9a-f]{64}$")))))) and
    (.closures.openopenRust | map(.identity) | length) == (.closures.openopenRust | map(.identity) | unique | length) and
    (.closures.codexRust | map(.identity) | length) == (.closures.codexRust | map(.identity) | unique | length) and
    ([.closures.openopenRust[],.closures.codexRust[]] | all(
      (.documents | map(.sourcePath) | length) == (.documents | map(.sourcePath) | unique | length))) and
    (.components | all(
      (.id|length)>0 and (.version|length)>0 and (.source|length)>0 and
      (.license|length)>0 and (.revision|length)>0 and (.documents|length)>0 and
      ((.documents | map(.sourcePath) | length) == (.documents | map(.sourcePath) | unique | length)))) and
    (.components | map(.id) | length) == (.components | map(.id) | unique | length) and
    ([.closures.openopenRust[] | select(.name=="serenity" and .version=="0.12.5" and (.source|contains($serenity)))] | length) == 1
  ' "$manifest" >/dev/null

  jq -r '([.closures.openopenRust[],.closures.codexRust[]] | .[].documents[].textSha256), (.components[].documents[].textSha256)' "$manifest" \
    | LC_ALL=C sort -u > "$TMP/referenced-hashes"
  find "$TEXTS" -type f -name '*.txt' -print | sed 's#.*/##; s/\.txt$//' | LC_ALL=C sort > "$TMP/stored-hashes"
  cmp "$TMP/referenced-hashes" "$TMP/stored-hashes"
  while IFS= read -r hash; do
    [ "$(hash_file "$TEXTS/$hash.txt")" = "$hash" ] || { printf 'text hash mismatch: %s\n' "$hash" >&2; exit 1; }
  done < "$TMP/stored-hashes"
}
validate_manifest "$WORK/manifest.json"

OPENOPEN_COUNT=$(jq '.closures.openopenRust|length' "$WORK/manifest.json")
CODEX_COUNT=$(jq '.closures.codexRust|length' "$WORK/manifest.json")
DOC_COUNT=$(jq '. as $m | ([.closures.openopenRust[],.closures.codexRust[]] | map(.documents|length) | add) as $rust | ([$m.components[].documents|length]|add) as $components | $rust+$components' "$WORK/manifest.json")
TEXT_COUNT=$(find "$TEXTS" -type f -name '*.txt' | wc -l | tr -d ' ')

GENERATED_DOC="$TMP/THIRD_PARTY_NOTICES.md"
printf '%s\n' \
  '# Third-Party Notices' \
  '' \
  'This file describes the deterministic notice payload for the current OpenOpen Friday-alpha distribution target. It is implementation and attribution evidence; it is not signing, notarization, provider, or release proof.' \
  '' \
  'The content-addressed payload is in `third_party/notices/manifest.json` and `third_party/notices/texts/<sha256>.txt`. Regenerate or verify it with `scripts/generate_third_party_notices.sh`; verification is offline and rejects closure drift, empty source/license fields, missing texts, hash mismatches, and duplicate package identities.' \
  '' \
  '## Generated closure' \
  '' \
  "- Target: \`$TARGET\`; dependency kinds: normal and build (development-only dependencies excluded)." \
  "- OpenOpen roots: \`openopen-host\` and \`openopen-effect-broker\`; $OPENOPEN_COUNT transitive third-party Rust package identities." \
  "- Codex roots: \`codex-cli\` and \`codex-code-mode-host\`; $CODEX_COUNT transitive third-party Rust package identities." \
  "- Notice documents: $DOC_COUNT references resolving to $TEXT_COUNT unique SHA-256-addressed text files." \
  '' \
  '## Exact runtime and source pins' \
  '' \
  "- OpenAI Codex app-server \`0.144.0\`: official source commit \`$CODEX_COMMIT\`, Apache-2.0. The OpenOpen protocol manifest maps the distributed package hashes to this exact source commit; the upstream \`codex-package.json\` does not itself record the source commit. The payload includes the root Apache-2.0 license, root NOTICE (including Ratatui attribution), and the normal/build closure for \`codex-cli\` plus \`codex-code-mode-host\`." \
  "- Bundled ripgrep \`15.1.0\`: source commit \`$RIPGREP_COMMIT\`, MIT OR Unlicense; \`COPYING\`, \`LICENSE-MIT\`, and \`UNLICENSE\` are included." \
  "- imsg \`0.13.0\`: dereferenced commit \`$IMSG_COMMIT\`, MIT, copyright 2026 Peter Steinberger. Its exact Swift pins and notices are Commander 0.2.4 at \`bd219c4ee9032fee3e009856f81fcc6ec09a85f4\`, PhoneNumberKit 5.0.4 at \`ab06a8333394f4a4fb6eecca447dae0aa06c1eca\`, and SQLite.swift 0.16.0 at \`964c300fb0736699ce945c9edb56ecd62eba27a3\` (all MIT)." \
  "- serenity \`0.12.5\`: exact commit \`$SERENITY_COMMIT\`, ISC, present exactly once in the OpenOpen Rust closure." \
  "- Friday contract source: immutable MIT commit \`$FRIDAY_COMMIT\`; its license text is included. OpenOpen ports the contract/test semantics and does not distribute Friday's TypeScript/Node runtime." \
  '' \
  '## Planned but not distributed' \
  '' \
  '`rust_xlsxwriter` for Hero C is planned after `FRIDAY_ALPHA_READY`; it is not part of this payload or current distribution closure. Its future notices must be generated from the then-locked closure before distribution.' \
  > "$GENERATED_DOC"

if [ "$CHECK" -eq 1 ]; then
  [ -d "$DEST" ] || { printf 'missing generated payload: %s\n' "$DEST" >&2; exit 1; }
  diff -ru "$DEST" "$WORK"
  cmp "$DOC" "$GENERATED_DOC"
  printf 'third-party notices verified: openopen=%s codex=%s documents=%s texts=%s\n' \
    "$OPENOPEN_COUNT" "$CODEX_COUNT" "$DOC_COUNT" "$TEXT_COUNT"
else
  rm -rf "$DEST"
  mkdir -p "$(dirname "$DEST")"
  cp -R "$WORK" "$DEST"
  cp "$GENERATED_DOC" "$DOC"
  printf 'third-party notices generated: openopen=%s codex=%s documents=%s texts=%s\n' \
    "$OPENOPEN_COUNT" "$CODEX_COUNT" "$DOC_COUNT" "$TEXT_COUNT"
fi
