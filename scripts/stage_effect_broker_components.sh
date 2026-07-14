#!/bin/zsh
set -euo pipefail

if [[ $# -ne 1 ]]; then
  print -u2 "usage: $0 /absolute/path/to/OpenOpen.app"
  exit 64
fi

app_bundle="$1"
if [[ "$app_bundle" != /* || ! -f "$app_bundle/Contents/Info.plist" ]]; then
  print -u2 "an existing absolute OpenOpen.app with Contents/Info.plist is required"
  exit 65
fi

script_dir="${0:A:h}"
repo_root="${script_dir:h}"
swift_package="$repo_root/macos/EffectBrokerBridge"

cargo build \
  --manifest-path "$repo_root/Cargo.toml" \
  --release \
  --package openopen-effect-broker \
  --bin OpenOpenEffectBrokerWorker

swift build \
  --package-path "$swift_package" \
  --configuration release \
  --product OpenOpenEffectBroker \
  -Xswiftc -warnings-as-errors

swift_bin="$(swift build --package-path "$swift_package" --configuration release --show-bin-path)"
daemon_source="$swift_bin/OpenOpenEffectBroker"
worker_source="$repo_root/target/release/OpenOpenEffectBrokerWorker"
plist_source="$swift_package/Sources/EffectBrokerBridge/Resources/LaunchDaemons/com.thesongzhu.OpenOpen.EffectBroker.plist"

install -d -m 0755 "$app_bundle/Contents/MacOS"
install -d -m 0755 "$app_bundle/Contents/Library/LaunchDaemons"
install -m 0755 "$daemon_source" "$app_bundle/Contents/MacOS/OpenOpenEffectBroker"
install -m 0755 "$worker_source" "$app_bundle/Contents/MacOS/OpenOpenEffectBrokerWorker"
install -m 0644 "$plist_source" \
  "$app_bundle/Contents/Library/LaunchDaemons/com.thesongzhu.OpenOpen.EffectBroker.plist"

plutil -lint \
  "$app_bundle/Contents/Library/LaunchDaemons/com.thesongzhu.OpenOpen.EffectBroker.plist"

print "STAGED_UNSIGNED_NOT_RELEASE_PROOF"
print "Sign the worker as com.thesongzhu.OpenOpen.EffectBroker.Worker, then the daemon as com.thesongzhu.OpenOpen.EffectBroker, then the containing app. Registration and acceptance remain blocked until signed/admin runtime proof exists."
