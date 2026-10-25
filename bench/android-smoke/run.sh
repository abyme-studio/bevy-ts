#!/usr/bin/env bash
# The Android smoke (change android-native, task 4.6): the engine library from scripts/android in a
# minimal Java host on a headless arm64 emulator (API 34, `-no-window`). The host loads the .so by
# path from its filesDir, plays a built game, and run.sh checks from logcat: started, a tap reaching
# the host's input, the surface dropped and replaced across a trip to the home screen, and exited.
# It records the first-frame time, the .so's stripped and gzipped sizes, the APK size and
# screenshots in bench/android-smoke/results/.
#
#   bench/android-smoke/run.sh [game]     (default arena; a game folder under examples/)
#
# Needs: dist/aarch64-linux-android/ (scripts/android), the SDK's build-tools 35, platform 34, the
# emulator and system-images;android-34;google_apis;arm64-v8a. Nothing shows on screen.
set -euo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd)
cd "$root"
game=${1:-arena}
here=bench/android-smoke
results=$here/results
ANDROID_HOME=${ANDROID_HOME:-/opt/homebrew/share/android-commandlinetools}
bt=$ANDROID_HOME/build-tools/35.0.0
jar=$ANDROID_HOME/platforms/android-34/android.jar
adb=$ANDROID_HOME/platform-tools/adb
cache=${BEVY_TS_SMOKE_CACHE:-$HOME/.cache/bevy-ts/android-smoke}
export ANDROID_AVD_HOME=$cache/avd
port=5584
serial=emulator-$port
pkg=dev.bevyts.smoke
so=dist/aarch64-linux-android/libbevy_ts_android.so
[[ -f "$so" ]] || { echo "run.sh: no $so; run scripts/android first" >&2; exit 1; }
mkdir -p "$cache" "$results" "$ANDROID_AVD_HOME"

say() { echo "smoke: $*"; }

# The APK: Engine.java and the activity against android.jar, dexed, linked, aligned and signed.
build=$cache/apk
rm -rf "$build" && mkdir -p "$build/classes" "$build/dex"
javac -source 11 -target 11 -nowarn -cp "$jar" -d "$build/classes" \
  dist/aarch64-linux-android/dev/bevyts/Engine.java $here/src/dev/bevyts/smoke/SmokeActivity.java 2>&1 | grep -v "bootstrap classpath" || true
"$bt/d8" --min-api 29 --lib "$jar" --output "$build/dex" $(find "$build/classes" -name '*.class')
"$bt/aapt2" link -I "$jar" --manifest $here/AndroidManifest.xml -o "$build/base.apk"
(cd "$build/dex" && zip -q ../base.apk classes.dex)
"$bt/zipalign" -f 4 "$build/base.apk" "$build/aligned.apk"
key=$cache/debug.keystore
[[ -f "$key" ]] || keytool -genkeypair -keystore "$key" -storepass android -keypass android -alias debug \
  -keyalg RSA -keysize 2048 -validity 10000 -dname "CN=bevy-ts smoke" >/dev/null 2>&1
"$bt/apksigner" sign --ks "$key" --ks-pass pass:android --out "$build/smoke.apk" "$build/aligned.apk"
apk=$build/smoke.apk

# The game, built by this checkout's engine; only its game/ folder goes to the phone.
out=$cache/game-$game
~/dev/abyme/scripts/gate-slot run bevy-ts:android-smoke -- cargo build -q -p bevy-ts-cli
./target/debug/bevy-ts build "examples/$game" --out "$out" >/dev/null

# A headless emulator of its own.
if [[ ! -d "$ANDROID_AVD_HOME/bevy-ts-smoke.avd" ]]; then
  echo no | "$ANDROID_HOME/cmdline-tools/latest/bin/avdmanager" -s create avd -n bevy-ts-smoke \
    -k "system-images;android-34;google_apis;arm64-v8a" -d pixel_6 >/dev/null
fi
"$ANDROID_HOME/emulator/emulator" -avd bevy-ts-smoke -port $port -no-window -no-audio -no-boot-anim \
  -no-snapshot -gpu swiftshader_indirect > "$cache/emulator.log" 2>&1 &
emu=$!
trap '"$adb" -s $serial emu kill >/dev/null 2>&1 || true; wait $emu 2>/dev/null || true' EXIT

# Waits until `$1` returns true, at most `$2` seconds; else fails naming `$3`.
until_ok() {
  local deadline=$((SECONDS + $2))
  until eval "$1"; do
    (( SECONDS < deadline )) || { echo "smoke: $3: not within $2 s" >&2; "$adb" -s $serial logcat -d -s bevy-ts-smoke:V bevy-ts:V RustStdoutStderr:V DEBUG:V AndroidRuntime:E > "$results/logcat-failed.txt" 2>&1 || true; exit 1; }
    sleep 1
  done
}
log() { "$adb" -s $serial logcat -d -s bevy-ts-smoke:I 2>/dev/null; }
logged() { log | grep -q -- "$1"; }

until_ok '[[ "$("$adb" -s $serial shell getprop sys.boot_completed 2>/dev/null | tr -d "\r")" == 1 ]]' 600 "the emulator booted"
say "booted"
"$adb" -s $serial shell input keyevent KEYCODE_WAKEUP
"$adb" -s $serial shell wm dismiss-keyguard >/dev/null 2>&1 || true
"$adb" -s $serial install -r -t "$apk" >/dev/null
"$adb" -s $serial exec-in run-as $pkg sh -c 'mkdir -p files && cat > files/libbevy_ts_android.so' < "$so"
"$adb" -s $serial shell run-as $pkg rm -rf files/games
tar -C "$out/game" -cf - . | "$adb" -s $serial exec-in run-as $pkg sh -c "mkdir -p files/games/$game && tar -C files/games/$game -xf -"
"$adb" -s $serial logcat -c

# Play: started from the library in filesDir.
"$adb" -s $serial shell am start -W -n $pkg/.SmokeActivity --es game "$game" >/dev/null
until_ok 'logged "\"event\":\"started\"" || logged "failed"' 180 "started"
logged "failed" && { log; exit 1; }
say "$(log | grep -o 'loaded .*' | head -1)"
say "$(log | grep -o 'first frame in .*' | head -1)"
sleep 5 # frames to look at, for the screenshot only
"$adb" -s $serial exec-out screencap -p > "$results/$game-playing.png"

# A tap reaches the host's input, which hands it to the engine.
read -r w h < <("$adb" -s $serial shell wm size | sed -n 's/.*: \([0-9]*\)x\([0-9]*\).*/\1 \2/p' | tail -1)
"$adb" -s $serial shell input tap $((w / 2)) $((h / 2))
until_ok 'logged "touch start" && logged "touch end"' 30 "the tap"
say "tap"

# Home drops the surface (the host waits for the engine to let go) and pauses; back again gives a
# new surface and resumes.
"$adb" -s $serial shell input keyevent KEYCODE_HOME
until_ok 'logged "surface released" && logged "paused"' 30 "the surface released"
say "$(log | grep -o 'surface released in .*' | tail -1)"
"$adb" -s $serial shell am start -W -n $pkg/.SmokeActivity >/dev/null
until_ok 'logged "resumed"' 30 "resumed"
sleep 5
"$adb" -s $serial exec-out screencap -p > "$results/$game-after-replace.png"
say "resumed on a new surface"

# Quit: the engine stops and its last event is exited.
"$adb" -s $serial shell am start -n $pkg/.SmokeActivity --ez quit true >/dev/null
until_ok 'logged "\"event\":\"exited\"" && logged "closed"' 60 "exited"
say "$(log | grep -o 'event {"event":"exited".*' | tail -1)"
"$adb" -s $serial logcat -d -s bevy-ts-smoke:V bevy-ts:V RustStdoutStderr:V AndroidRuntime:E > "$results/$game-logcat.txt" 2>&1 || true

size=$(wc -c < "$so")
gz=$(gzip -9c "$so" | wc -c)
apk_size=$(wc -c < "$apk")
first=$(log | grep -o 'first frame in [0-9]* ms' | head -1 | grep -o '[0-9]*')
{
  echo "# Android smoke: $game"
  echo
  echo "$(date -u +%Y-%m-%dT%H:%MZ), emulator API 34 arm64 (SwiftShader), bevy-ts $(log | grep -o 'version [^ ]*' | head -1 | cut -d' ' -f2)."
  echo
  echo "| | |"
  echo "|---|---|"
  echo "| .so, stripped | $((size / 1000)) kB |"
  echo "| .so, gzip -9 | $((gz / 1000)) kB |"
  echo "| smoke APK, no .so | $((apk_size / 1000)) kB |"
  echo "| create to first frame | $first ms |"
  echo
  echo "Loaded from filesDir by path, started, tap, surface released and replaced, exited: all seen."
} > "$results/$game.md"
cat "$results/$game.md"
