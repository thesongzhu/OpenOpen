#!/bin/bash
set -euo pipefail
umask 077

usage() {
  echo "usage: $0 --source-root ABSOLUTE_PATH --output ABSOLUTE_RUNTIME_DIRECTORY --receipt ABSOLUTE_PATH" >&2
  exit 64
}

source_root=""
output=""
receipt=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --source-root) [[ $# -ge 2 ]] || usage; source_root="$2"; shift 2 ;;
    --output) [[ $# -ge 2 ]] || usage; output="$2"; shift 2 ;;
    --receipt) [[ $# -ge 2 ]] || usage; receipt="$2"; shift 2 ;;
    *) usage ;;
  esac
done

[[ "$source_root" = /* && "$output" = /* && "$receipt" = /* ]] || usage
[[ "$output" != "/" && "$receipt" != "/" && "$output" != "$receipt" ]] || usage
[[ "$(git -C "$source_root" rev-parse --is-inside-work-tree 2>/dev/null)" == "true" ]] || {
  echo "imsg source root must be a Git checkout" >&2
  exit 65
}
[[ ! -e "$output" ]] || {
  echo "refusing to overwrite existing runtime tree: $output" >&2
  exit 65
}
[[ ! -e "$receipt" ]] || {
  echo "refusing to overwrite existing receipt: $receipt" >&2
  exit 65
}

readonly expected_commit="fa2f82d7dbda4c802d91c1d41bb6c53564ed2fdc"
readonly expected_lock_sha="642390f861e9581bc0ec6e4b43abfb18bbbb20e37e7b130c35832a0e50b66054"
repo_root="$(cd "$(dirname "$0")/.." && pwd -P)"
patch_path="$repo_root/third_party/imsg/openopen-basic-rpc.patch"
lock_path="$repo_root/third_party/imsg/Package.resolved"

[[ "$(git -C "$source_root" rev-parse HEAD)" == "$expected_commit" ]] || {
  echo "imsg source checkout is not the approved commit" >&2
  exit 66
}
[[ -z "$(git -C "$source_root" status --porcelain)" ]] || {
  echo "imsg source checkout is not clean" >&2
  exit 66
}
actual_lock_sha="$(/usr/bin/shasum -a 256 "$lock_path" | /usr/bin/awk '{print $1}')"
[[ "$actual_lock_sha" == "$expected_lock_sha" ]] || {
  echo "tracked imsg dependency lock hash mismatch" >&2
  exit 66
}

output_parent="$(dirname "$output")"
mkdir -p "$output_parent"
output_parent="$(cd "$output_parent" && pwd -P)"
output="$output_parent/$(basename "$output")"
receipt_parent="$(dirname "$receipt")"
mkdir -p "$receipt_parent"
receipt_parent="$(cd "$receipt_parent" && pwd -P)"
receipt="$receipt_parent/$(basename "$receipt")"
case "$receipt" in
  "$output"/*)
    echo "receipt must be outside the runtime tree" >&2
    exit 65
    ;;
esac

staging="/private/tmp/OpenOpen-imsg-pinned-build"
[[ ! -e "$staging" ]] || {
  echo "refusing to reuse pinned imsg build root: $staging" >&2
  exit 65
}
mkdir "$staging"
claimed_output=0
claimed_receipt=0
cleanup() {
  rm -rf "$staging"
  [[ "$claimed_receipt" -eq 0 ]] || rm -f "$receipt"
  [[ "$claimed_output" -eq 0 ]] || rm -rf "$output"
}
trap cleanup EXIT

git -C "$source_root" archive "$expected_commit" | /usr/bin/tar -x -C "$staging"
/usr/bin/ditto "$lock_path" "$staging/Package.resolved"
git -C "$staging" apply --check "$patch_path"
git -C "$staging" apply "$patch_path"

swift test \
  --package-path "$staging" \
  --scratch-path "$staging/.build-openopen-tests" \
  --filter OpenOpen \
  --disable-automatic-resolution \
  -Xswiftc -warnings-as-errors
swift build \
  --package-path "$staging" \
  --scratch-path "$staging/.build-openopen" \
  --configuration release \
  --product imsg \
  --disable-automatic-resolution \
  -Xswiftc -warnings-as-errors

bin_dir="$(swift build \
  --package-path "$staging" \
  --scratch-path "$staging/.build-openopen" \
  --configuration release \
  --show-bin-path \
  --disable-automatic-resolution)"
binary="$bin_dir/imsg"
resource_source="$bin_dir/PhoneNumberKit_PhoneNumberKit.bundle"
[[ -x "$binary" && -d "$resource_source" ]] || {
  echo "pinned imsg build is missing its binary or PhoneNumberKit resource bundle" >&2
  exit 67
}
[[ "$("$binary" --version)" == "0.13.0" ]] || {
  echo "pinned imsg executable reports an unexpected version" >&2
  exit 67
}

expected_core_sources="$staging/expected-core-sources.txt"
expected_cli_sources="$staging/expected-cli-sources.txt"
/usr/bin/printf '%s\n' \
  Sources/IMsgCore/DatabaseIDs.swift \
  Sources/IMsgCore/Errors.swift \
  Sources/IMsgCore/IMsgEventTailer.swift \
  Sources/IMsgCore/ISO8601.swift \
  Sources/IMsgCore/Message+URLPreview.swift \
  Sources/IMsgCore/MessageFilter.swift \
  Sources/IMsgCore/MessagePolls.swift \
  Sources/IMsgCore/MessageSendStatus.swift \
  Sources/IMsgCore/MessageStore+Chats.swift \
  Sources/IMsgCore/MessageStore+Helpers.swift \
  Sources/IMsgCore/MessageStore+MessageConstruction.swift \
  Sources/IMsgCore/MessageStore+MessageRows.swift \
  Sources/IMsgCore/MessageStore+Messages.swift \
  Sources/IMsgCore/MessageStore+Polls.swift \
  Sources/IMsgCore/MessageStore+Queries.swift \
  Sources/IMsgCore/MessageStore+ReplyContext.swift \
  Sources/IMsgCore/MessageStore+SQLRow.swift \
  Sources/IMsgCore/MessageStore+URLPreviews.swift \
  Sources/IMsgCore/MessageStore.swift \
  Sources/IMsgCore/MessageStoreSchema.swift \
  Sources/IMsgCore/MessageWatcher.swift \
  Sources/IMsgCore/Models.swift \
  Sources/IMsgCore/OpenOpenBasicMessageSender.swift \
  Sources/IMsgCore/OpenOpenRuntimeResourceProbe.swift \
  Sources/IMsgCore/TypedStreamParser.swift | LC_ALL=C sort >"$expected_core_sources"
/usr/bin/printf '%s\n' \
  Sources/imsg/CommandOutputEmittedError.swift \
  Sources/imsg/CommandRouter.swift \
  Sources/imsg/CommandSignatures.swift \
  Sources/imsg/CommandSpec.swift \
  Sources/imsg/Commands/RpcCommand.swift \
  Sources/imsg/HelpPrinter.swift \
  Sources/imsg/IMsgCLI.swift \
  Sources/imsg/OpenOpenBasicRPCServer.swift \
  Sources/imsg/RPCRequestParser.swift \
  Sources/imsg/RPCStartupErrorServer.swift \
  Sources/imsg/RuntimeOptions.swift \
  Sources/imsg/StdoutWriter.swift \
  Sources/imsg/Version.swift | LC_ALL=C sort >"$expected_cli_sources"

actual_core_sources="$staging/actual-core-sources.txt"
actual_cli_sources="$staging/actual-cli-sources.txt"
/usr/bin/sed "s#^$staging/##" "$bin_dir/IMsgCore.build/sources" | LC_ALL=C sort >"$actual_core_sources"
/usr/bin/sed "s#^$staging/##" "$bin_dir/imsg.build/sources" | LC_ALL=C sort >"$actual_cli_sources"
/usr/bin/cmp -s "$expected_core_sources" "$actual_core_sources" || {
  echo "compiled IMsgCore source manifest differs from the approved whitelist" >&2
  /usr/bin/diff -u "$expected_core_sources" "$actual_core_sources" >&2 || true
  exit 67
}
/usr/bin/cmp -s "$expected_cli_sources" "$actual_cli_sources" || {
  echo "compiled imsg source manifest differs from the approved whitelist" >&2
  /usr/bin/diff -u "$expected_cli_sources" "$actual_cli_sources" >&2 || true
  exit 67
}

help="$("$binary" --help)"
[[ "$help" == *"rpc"* ]] || {
  echo "pinned imsg executable does not expose rpc" >&2
  exit 67
}
for excluded in launch typing send-rich send-attachment chat-create tcp server; do
  [[ "$help" != *"$excluded"* ]] || {
    echo "pinned imsg executable exposes excluded surface: $excluded" >&2
    exit 67
  }
done

readonly forbidden_pattern='IMCoreBridge|IMsgBridge(Client|Protocol)|MessagesLauncher|BridgeHelperLocator|DYLD_INSERT_LIBRARIES|PrivateFrameworks/IMCore|csrutil disable|imsg-bridge-helper'
if /usr/bin/strings -a "$binary" | /usr/bin/grep -E "$forbidden_pattern" >/dev/null; then
  echo "pinned imsg executable contains a forbidden private/bridge marker" >&2
  exit 67
fi
if /usr/bin/nm -gj "$binary" 2>/dev/null | /usr/bin/grep -E 'IMCoreBridge|IMsgBridge|MessagesLauncher|BridgeHelper' >/dev/null; then
  echo "pinned imsg executable exports a forbidden private/bridge symbol" >&2
  exit 67
fi
if /usr/bin/otool -L "$binary" | /usr/bin/grep -E 'PrivateFramework|IMCore' >/dev/null; then
  echo "pinned imsg executable links a forbidden private framework" >&2
  exit 67
fi

mkdir -p "$output/bin"
claimed_output=1
/usr/bin/ditto "$binary" "$output/bin/imsg"
chmod 0755 "$output/bin/imsg"
/usr/bin/ditto "$resource_source" "$output/bin/PhoneNumberKit_PhoneNumberKit.bundle"

[[ -z "$(/usr/bin/find -P "$output" -type l -print -quit)" ]] || {
  echo "runtime tree contains a symbolic link" >&2
  exit 67
}
[[ -z "$(/usr/bin/find -P "$output" ! -type d ! -type f -print -quit)" ]] || {
  echo "runtime tree contains a non-regular entry" >&2
  exit 67
}
actual_runtime_files="$staging/runtime-files.txt"
(
  cd "$output"
  /usr/bin/find -P . -type f -print | /usr/bin/sed 's#^./##' | LC_ALL=C sort
) >"$actual_runtime_files"
/usr/bin/printf '%s\n' \
  bin/PhoneNumberKit_PhoneNumberKit.bundle/PhoneNumberMetadata.json \
  bin/PhoneNumberKit_PhoneNumberKit.bundle/PrivacyInfo.xcprivacy \
  bin/imsg | LC_ALL=C sort >"$staging/expected-runtime-files.txt"
/usr/bin/cmp -s "$staging/expected-runtime-files.txt" "$actual_runtime_files" || {
  echo "runtime tree contains an unexpected or missing file" >&2
  /usr/bin/diff -u "$staging/expected-runtime-files.txt" "$actual_runtime_files" >&2 || true
  exit 67
}

# Prove the copied resource is sufficient by hiding SwiftPM's build-tree fallback.
mv "$resource_source" "$staging/PhoneNumberKit_PhoneNumberKit.bundle.hidden"
probe_db="$staging/probe.sqlite"
/usr/bin/sqlite3 "$probe_db" 'PRAGMA user_version = 1;'
probe_response="$(/usr/bin/printf '%s\n' \
  '{"jsonrpc":"2.0","id":"probe","method":"private.unavailable","params":{}}' \
  | "$output/bin/imsg" rpc --db "$probe_db")"
/usr/bin/printf '%s\n' "$probe_response" \
  | /usr/bin/jq -e '.id == "probe" and .error.code == -32601' >/dev/null || {
    echo "staged imsg failed its resource-isolated basic RPC probe" >&2
    exit 67
  }

artifact_sha="$(/usr/bin/shasum -a 256 "$output/bin/imsg" | /usr/bin/awk '{print $1}')"
artifact_size="$(/usr/bin/stat -f '%z' "$output/bin/imsg")"
patch_sha="$(/usr/bin/shasum -a 256 "$patch_path" | /usr/bin/awk '{print $1}')"
core_sources_sha="$(/usr/bin/shasum -a 256 "$actual_core_sources" | /usr/bin/awk '{print $1}')"
cli_sources_sha="$(/usr/bin/shasum -a 256 "$actual_cli_sources" | /usr/bin/awk '{print $1}')"
core_sources_json="$(/usr/bin/jq -R -s 'split("\n") | map(select(length > 0))' "$actual_core_sources")"
cli_sources_json="$(/usr/bin/jq -R -s 'split("\n") | map(select(length > 0))' "$actual_cli_sources")"

resource_manifest="$staging/resource-manifest.txt"
resource_records="$staging/resource-records.jsonl"
: >"$resource_manifest"
: >"$resource_records"
while IFS= read -r relative; do
  case "$relative" in
    bin/PhoneNumberKit_PhoneNumberKit.bundle/*)
      file="$output/$relative"
      size="$(/usr/bin/stat -f '%z' "$file")"
      sha="$(/usr/bin/shasum -a 256 "$file" | /usr/bin/awk '{print $1}')"
      /usr/bin/printf '%s\t%s\t%s\n' "$relative" "$size" "$sha" >>"$resource_manifest"
      /usr/bin/jq -nc \
        --arg path "$relative" \
        --arg sha256 "$sha" \
        --argjson size "$size" \
        '{path: $path, size: $size, sha256: $sha256}' >>"$resource_records"
      ;;
  esac
done <"$actual_runtime_files"
resource_tree_sha="$(/usr/bin/shasum -a 256 "$resource_manifest" | /usr/bin/awk '{print $1}')"
resource_files_json="$(/usr/bin/jq -s . "$resource_records")"

runtime_manifest="$staging/runtime-manifest.txt"
: >"$runtime_manifest"
while IFS= read -r relative; do
  file="$output/$relative"
  size="$(/usr/bin/stat -f '%z' "$file")"
  sha="$(/usr/bin/shasum -a 256 "$file" | /usr/bin/awk '{print $1}')"
  /usr/bin/printf '%s\t%s\t%s\n' "$relative" "$size" "$sha" >>"$runtime_manifest"
done <"$actual_runtime_files"
runtime_tree_sha="$(/usr/bin/shasum -a 256 "$runtime_manifest" | /usr/bin/awk '{print $1}')"

claimed_receipt=1
/usr/bin/jq -n \
  --arg component "openclaw/imsg" \
  --arg version "0.13.0" \
  --arg source_commit "$expected_commit" \
  --arg patch_sha256 "$patch_sha" \
  --arg package_resolved_sha256 "$expected_lock_sha" \
  --arg surface "openopen-basic-json-rpc-stdio" \
  --arg binary_sha256 "$artifact_sha" \
  --argjson binary_size "$artifact_size" \
  --arg runtime_tree_sha256 "$runtime_tree_sha" \
  --arg resource_tree_sha256 "$resource_tree_sha" \
  --arg core_sources_sha256 "$core_sources_sha" \
  --arg cli_sources_sha256 "$cli_sources_sha" \
  --argjson resource_files "$resource_files_json" \
  --argjson core_sources "$core_sources_json" \
  --argjson cli_sources "$cli_sources_json" \
  '{
    schemaVersion: 2,
    component: $component,
    version: $version,
    sourceCommit: $source_commit,
    patchSha256: $patch_sha256,
    packageResolvedSha256: $package_resolved_sha256,
    surface: $surface,
    runtimeTreeSha256: $runtime_tree_sha256,
    binary: {path: "bin/imsg", size: $binary_size, sha256: $binary_sha256},
    resources: {
      bundlePath: "bin/PhoneNumberKit_PhoneNumberKit.bundle",
      treeSha256: $resource_tree_sha256,
      files: $resource_files
    },
    compiledSources: {
      IMsgCore: {manifestSha256: $core_sources_sha256, files: $core_sources},
      imsg: {manifestSha256: $cli_sources_sha256, files: $cli_sources}
    }
  }' >"$receipt"
/usr/bin/jq -e \
  '.schemaVersion == 2
   and .binary.path == "bin/imsg"
   and (.resources.files | length) == 2
   and (.compiledSources.IMsgCore.files | length) == 25
   and (.compiledSources.imsg.files | length) == 13' \
  "$receipt" >/dev/null
claimed_receipt=0
claimed_output=0
echo "PINNED_IMSG_BASIC_RPC_V2 $artifact_sha $runtime_tree_sha $output $receipt"
