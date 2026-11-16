#!/usr/bin/env bash
# Builds the iPad app and puts it on an iPad: cross-compiles `orb-ios` for aarch64-apple-ios,
# wraps the binary in OpenRB-338.app, signs it with an Apple Development identity and the
# team's wildcard provisioning profile, and installs it with devicectl.
#
#   scripts/ios-app.sh [--run] [--no-install]
#
# --run starts the app after installing it; --no-install stops after signing (the bundle stays
# in dist/ios/OpenRB-338.app). What it signs with:
#
#   ORB_IOS_DEVICE    the iPad: a devicectl UDID, or a name it knows (default: the first
#                     paired iPad in `xcrun devicectl list devices`)
#   ORB_IOS_BUNDLE_ID the bundle id (default com.rnsksoftronics.openrb338; the profile must
#                     cover it, e.g. the team's `team.*` wildcard)
#   ORB_IOS_SIGN      the signing identity (default: the Apple Development identity the
#                     chosen profile lists)
#   ORB_IOS_PROFILE   the provisioning profile (default: the newest unexpired profile of the
#                     team that names the device and a wildcard App ID)
#
# Needs Xcode (the iPadOS SDK), rustup's target `aarch64-apple-ios` (rustup target add
# aarch64-apple-ios) and the iPad paired with this Mac.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
target=aarch64-apple-ios
app=$root/dist/ios/OpenRB-338.app
bundle=${ORB_IOS_BUNDLE_ID:-com.rnsksoftronics.openrb338}
exe=OpenRB-338
run=0
install=1
for arg in "$@"; do
    case $arg in
        --run) run=1 ;;
        --no-install) install=0 ;;
        *) echo "usage: scripts/ios-app.sh [--run] [--no-install]" >&2; exit 2 ;;
    esac
done

command -v cargo >/dev/null || { echo "no cargo: install rustup" >&2; exit 1; }
xcrun --sdk iphoneos --show-sdk-path >/dev/null 2>&1 || {
    echo "no iPadOS SDK: install Xcode" >&2
    exit 1
}
device=${ORB_IOS_DEVICE:-}
if [ -z "$device" ] && [ "$install" = 1 ]; then
    device=$(xcrun devicectl list devices 2>/dev/null | awk '
        /available \(paired\)/ && /iPad/ {
            for (i = 1; i <= NF; i++) if ($i ~ /^[0-9A-F]{8}-[0-9A-F]{16}$/) { print $i; exit }
        }')
    [ -n "$device" ] || {
        echo "no paired iPad: connect it, trust this Mac, or set ORB_IOS_DEVICE" >&2
        exit 1
    }
fi

echo "ios: building orb-ios for $target"
# rustc links the target's own minimum (iOS 10.0) unless this is set, and the C parts of the
# engine (the vendored vorbis) then call ___chkstk_darwin, which libSystem only has above it.
export IPHONEOS_DEPLOYMENT_TARGET=${IPHONEOS_DEPLOYMENT_TARGET:-15.0}
cargo build --release --target "$target" -p orb-ios --manifest-path "$root/Cargo.toml"

# The profile picks the team and the identity: the App ID prefix is the team, and only a
# certificate the profile lists may sign the app.
profiles=$(mktemp)
trap 'rm -f "$profiles"' EXIT
python3 - "$device" "$profiles" <<'PY'
import datetime, glob, hashlib, os, plistlib, subprocess, sys

device, out = sys.argv[1], sys.argv[2]
ids = {}
for line in subprocess.run(["security", "find-identity", "-v", "-p", "codesigning"],
                           capture_output=True, text=True).stdout.splitlines():
    if "Apple Development:" not in line:
        continue
    sha = line.split()[1]
    name = line.split('"')[1]
    ids[sha] = name
now = datetime.datetime.now(datetime.timezone.utc).replace(tzinfo=None)
best = None
pattern = os.path.expanduser(
    "~/Library/Developer/Xcode/UserData/Provisioning Profiles/*.mobileprovision")
for path in glob.glob(pattern):
    raw = subprocess.run(["security", "cms", "-D", "-i", path],
                         capture_output=True).stdout
    try:
        d = plistlib.loads(raw)
    except Exception:
        continue
    appid = d.get("Entitlements", {}).get("application-identifier", "")
    if not appid.endswith(".*"):
        continue
    if device and device.upper() not in [x.upper() for x in d.get("ProvisionedDevices", [])]:
        continue
    if d.get("ExpirationDate", now) <= now:
        continue
    certs = {hashlib.sha1(c).hexdigest().upper(): ids.get(hashlib.sha1(c).hexdigest().upper())
             for c in d.get("DeveloperCertificates", [])}
    certs = {sha: name for sha, name in certs.items() if name}
    if not certs:
        continue
    if best is None or d["ExpirationDate"] > best["ExpirationDate"]:
        best = dict(d, certs=certs)
if best is None:
    print("no unexpired wildcard profile lists this device and an installed Apple "
          "Development identity; open the account in Xcode once", file=sys.stderr)
    sys.exit(1)
with open(out, "w") as f:
    cert = sorted(best["certs"])[0]
    f.write("\t".join([
        best["Entitlements"]["com.apple.developer.team-identifier"],
        best["certs"][cert],
        best["UUID"],
    ]) + "\n")
    print(best["Name"])
PY
IFS=$'\t' read -r team name uuid < "$profiles"
[ -n "${ORB_IOS_SIGN:-}" ] || ORB_IOS_SIGN=$name
if [ -n "${ORB_IOS_PROFILE:-}" ]; then
    profile=$ORB_IOS_PROFILE
else
    profile="$HOME/Library/Developer/Xcode/UserData/Provisioning Profiles/$uuid.mobileprovision"
fi
[ -f "$profile" ] || { echo "no provisioning profile at $profile" >&2; exit 1; }

rm -rf "$app"
mkdir -p "$app"
cp "$root/target/$target/release/orb-ios" "$app/$exe"
sed -e "s|@EXECUTABLE@|$exe|" -e "s|@BUNDLE_ID@|$bundle|" \
    -e "s|@VERSION@|$("$root/scripts/version.sh")|" -e "s|@BUILD@|1|" \
    -e "s|@MIN_OS@|$IPHONEOS_DEPLOYMENT_TARGET|" "$root/ios/Info.plist.in" > "$app/Info.plist"
cp "$profile" "$app/embedded.mobileprovision"

# The app icon: one 1024 picture (ios/Assets.xcassets) that actool turns into the icon set the
# system reads, and the Info.plist keys that name it.
partial=$(mktemp)
xcrun actool "$root/ios/Assets.xcassets" --compile "$app" --platform iphoneos \
    --minimum-deployment-target "$IPHONEOS_DEPLOYMENT_TARGET" --app-icon AppIcon \
    --output-partial-info-plist "$partial" --errors --warnings >/dev/null
/usr/libexec/PlistBuddy -c "Merge $partial" "$app/Info.plist" >/dev/null
rm -f "$partial"

ent=$(mktemp)
trap 'rm -f "$profiles" "$ent"' EXIT
cat > "$ent" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>application-identifier</key>
    <string>$team.$bundle</string>
    <key>com.apple.developer.team-identifier</key>
    <string>$team</string>
    <key>get-task-allow</key>
    <true/>
    <key>keychain-access-groups</key>
    <array>
        <string>$team.*</string>
    </array>
</dict>
</plist>
PLIST

echo "ios: signing $bundle with $ORB_IOS_SIGN (team $team)"
codesign --force --sign "$ORB_IOS_SIGN" --entitlements "$ent" --timestamp=none \
    --generate-entitlement-der "$app"
codesign --verify --strict "$app"

if [ "$install" = 1 ]; then
    echo "ios: installing on $device"
    xcrun devicectl device install app --device "$device" "$app"
    if [ "$run" = 1 ]; then
        xcrun devicectl device process launch --device "$device" "$bundle"
    fi
else
    echo "ios: $app"
fi
