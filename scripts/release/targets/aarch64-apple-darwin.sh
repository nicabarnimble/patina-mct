#!/usr/bin/env bash
set -euo pipefail

command_name=${1:-}
case $command_name in
  assemble)
    binary=${2:?missing binary}
    payload=${3:?missing payload directory}
    version=${4:?missing version}
    [[ $(uname -s) == Darwin && $(uname -m) == arm64 ]] || {
      printf 'aarch64-apple-darwin release assembly requires macOS arm64\n' >&2
      exit 1
    }
    mode=${MCT_RELEASE_SIGNING_MODE:-adhoc}
    if [[ $mode != adhoc ]]; then
      identity=${MCT_APPLE_SIGNING_IDENTITY:-}
      profile=${MCT_NOTARYTOOL_KEYCHAIN_PROFILE:-}
      if [[ -z $identity || -z $profile ]]; then
        printf 'notarized mode requires both named signing credentials\n' >&2
      else
        printf 'notarization execution is a named but unavailable R3 slot\n' >&2
      fi
      exit 1
    fi
    app="$payload/mct-daemon.app"
    executable="$app/Contents/MacOS/mct-daemon"
    mkdir -p "$(dirname "$executable")"
    install -m 0755 "$binary" "$executable"
    cat > "$app/Contents/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "https://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleExecutable</key><string>mct-daemon</string>
  <key>CFBundleIdentifier</key><string>io.patina.mct.mother</string>
  <key>CFBundleName</key><string>mct-daemon</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>${version}</string>
  <key>CFBundleVersion</key><string>${version}</string>
  <key>LSBackgroundOnly</key><true/>
</dict>
</plist>
EOF
    chmod 0644 "$app/Contents/Info.plist"
    /usr/bin/plutil -lint "$app/Contents/Info.plist" >/dev/null
    /usr/bin/codesign --force --sign - --timestamp=none "$app"
    "$0" verify "$payload"
    ;;
  verify)
    payload=${2:?missing payload directory}
    app="$payload/mct-daemon.app"
    /usr/bin/codesign --verify --strict --verbose=2 "$app"
    mapfile_output=$(find "$app/Contents/_CodeSignature" -mindepth 1 -maxdepth 1 -print)
    if [[ $mapfile_output != "$app/Contents/_CodeSignature/CodeResources" ]]; then
      printf 'signed bundle contains a non-contract _CodeSignature member\n' >&2
      exit 1
    fi
    [[ -f "$app/Contents/_CodeSignature/CodeResources" && ! -L "$app/Contents/_CodeSignature/CodeResources" ]] || {
      printf 'signed bundle has no regular CodeResources member\n' >&2
      exit 1
    }
    ;;
  notarization-plan)
    identity=${2:?missing Developer ID identity placeholder}
    profile=${3:?missing notarytool keychain profile placeholder}
    cat <<EOF
/usr/bin/codesign --force --options runtime --timestamp --sign '$identity' '<payload>/mct-daemon.app'
/usr/bin/ditto -c -k --keepParent '<payload>/mct-daemon.app' '<temporary-submission>.zip'
/usr/bin/xcrun notarytool submit '<temporary-submission>.zip' --keychain-profile '$profile' --wait
/usr/bin/xcrun stapler staple '<payload>/mct-daemon.app'
/usr/bin/xcrun stapler validate '<payload>/mct-daemon.app'
EOF
    ;;
  *)
    printf 'usage: %s assemble <binary> <payload-dir> <version> | verify <payload-dir> | notarization-plan <identity> <profile>\n' "$0" >&2
    exit 2
    ;;
esac
