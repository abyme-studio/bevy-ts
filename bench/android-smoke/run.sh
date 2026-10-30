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
# SMOKE_PROJECT (a project folder, else examples/<game>), SMOKE_ENTRY (its entry, else the project's
# own) and SMOKE_ENV (the engine's env, JSON) play another game under the name <game>; SETTLE is the
# seconds to wait before each screenshot (5), STARTED_WITHIN the seconds the first frame may take
# (180), SMOKE_GPU the emulator's `-gpu` (swiftshader_indirect; `host` renders on the Mac's GPU,
# still with no window). abyme-game.mjs plays agent town's worlds this way.
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
# Its linker warnings (tens of kB) go to a file: a large stderr through gate-slot has stalled it.
~/dev/abyme/scripts/gate-slot run bevy-ts:android-smoke -- bash -c 'cargo build -q -p bevy-ts-cli 2> target/android-smoke-cli.log' \
  || { grep -v ld64 target/android-smoke-cli.log >&2; exit 1; }
project=${SMOKE_PROJECT:-examples/$game}
# --copy-sources: the phone cannot read an asset source on this Mac (agent town's `~/` stardew).
# --entry is read from the working directory, so SMOKE_ENTRY (the project's) is put under it.
./target/debug/bevy-ts build "$project" --out "$out" --copy-sources ${SMOKE_ENTRY:+--entry "$project/$SMOKE_ENTRY"} >/dev/null
env64=$(printf '%s' "${SMOKE_ENV:-{\}}" | base64 | tr -d '\n')
settle=${SETTLE:-5}

# A headless emulator of its own, under a grant of the machine's budget held while run.sh lives:
# beside load 130-180 its CPUs hung 20 s at a time, its boot took 9 min and it crashed. Not wiped:
# a first boot is slower still, and run.sh clears the game it pushed before.
~/dev/abyme/scripts/gate-slot take $$ bevy-ts:android-emulator
if [[ ! -d "$ANDROID_AVD_HOME/bevy-ts-smoke.avd" ]]; then
  echo no | "$ANDROID_HOME/cmdline-tools/latest/bin/avdmanager" -s create avd -n bevy-ts-smoke \
    -k "system-images;android-34;google_apis;arm64-v8a" -d pixel_6 >/dev/null
fi
"$ANDROID_HOME/emulator/emulator" -avd bevy-ts-smoke -port $port -no-window -no-audio -no-boot-anim \
  -no-snapshot -gpu "${SMOKE_GPU:-swiftshader_indirect}" -memory 4096 -partition-size 16384 > "$cache/emulator.log" 2>&1 &
emu=$!
# Stopped within 30 s: a hung emulator answers no `emu kill`, and waiting on it held run.sh a day.
stop_emulator() {
  timeout 10 "$adb" -s $serial emu kill >/dev/null 2>&1 || true
  for _ in $(seq 30); do kill -0 $emu 2>/dev/null || break; sleep 1; done
  kill -9 $emu 2>/dev/null || true
  wait $emu 2>/dev/null || true
}
trap stop_emulator EXIT

# Waits until `$1` returns true, at most `$2` seconds; else fails naming `$3`.
until_ok() {
  local deadline=$((SECONDS + $2))
  until eval "$1"; do
    (( SECONDS < deadline )) || { echo "smoke: $3: not within $2 s" >&2; app_log > "$results/logcat-failed.txt"; exit 1; }
    sleep 1
  done
}
log() { "$adb" -s $serial logcat -d -s bevy-ts-smoke:I 2>/dev/null; }
# Everything the app logged, Bevy's own lines too (tagged by module, so not by one tag), and crashes.
# By the pid seen at start: the app may be gone by then.
app_pid=
app_log() {
  { [[ -n "$app_pid" ]] && "$adb" -s $serial logcat -d --pid="$app_pid"; "$adb" -s $serial logcat -d -s DEBUG:V AndroidRuntime:E; "$adb" -s $serial logcat -d -b crash; } 2>&1 || true
}
logged() { log | grep -q -- "$1"; }
count() { log | grep -c -- "$1" || true; }
# A loaded emulator shows "System UI isn't responding", over the game and in the way of a tap.
close_dialogs() { "$adb" -s $serial shell am broadcast -a android.intent.action.CLOSE_SYSTEM_DIALOGS >/dev/null 2>&1 || true; }

until_ok '[[ "$("$adb" -s $serial shell getprop sys.boot_completed 2>/dev/null | tr -d "\r")" == 1 ]]' 900 "the emulator booted"
say "booted"
"$adb" -s $serial shell input keyevent KEYCODE_WAKEUP
"$adb" -s $serial shell wm dismiss-keyguard >/dev/null 2>&1 || true
"$adb" -s $serial install -r -t "$apk" >/dev/null
# adb exec-in can cut a long stream short: push, copy inside the device, and check the size.
"$adb" -s $serial push -q "$so" /data/local/tmp/libbevy_ts_android.so >/dev/null
"$adb" -s $serial shell "cat /data/local/tmp/libbevy_ts_android.so | run-as $pkg sh -c 'mkdir -p files && cat > files/libbevy_ts_android.so'"
copied=$("$adb" -s $serial shell run-as $pkg stat -c %s files/libbevy_ts_android.so | tr -d '\r')
[[ "$copied" == "$(wc -c < "$so" | tr -d ' ')" ]] || { echo "smoke: the .so copied as $copied bytes" >&2; exit 1; }
"$adb" -s $serial shell run-as $pkg rm -rf files/games
tar -C "$out/game" -cf "$cache/game.tar" .
"$adb" -s $serial push -q "$cache/game.tar" /data/local/tmp/game.tar >/dev/null
"$adb" -s $serial shell "cat /data/local/tmp/game.tar | run-as $pkg sh -c 'mkdir -p files/games/$game && tar -C files/games/$game -xf -' && rm /data/local/tmp/game.tar"
say "$("$adb" -s $serial shell df -h /data | tail -1 | awk '{ print $2 " size, " $3 " used, " $4 " free on /data" }')"
# A large game's push can leave Android restarting its system server: wait for it to be back.
until_ok '"$adb" -s $serial shell service check activity 2>/dev/null | grep -q ": found"' 300 "the activity manager is up"
"$adb" -s $serial logcat -G 16M >/dev/null 2>&1 || true
"$adb" -s $serial logcat -c

# Play: started from the library in filesDir.
"$adb" -s $serial shell am start -W -n $pkg/.SmokeActivity --es game "$game" --es env64 "$env64" >/dev/null
app_pid=$("$adb" -s $serial shell pidof $pkg | tr -d '\r')
until_ok 'logged "\"event\":\"started\"" || logged "failed"' "${STARTED_WITHIN:-180}" "started"
logged "failed" && { log; exit 1; }
say "$(log | grep -o 'loaded .*' | head -1)"
say "$(log | grep -o 'first frame in .*' | head -1)"
sleep "$settle" # frames to look at, for the screenshot only
close_dialogs
"$adb" -s $serial exec-out screencap -p > "$results/$game-playing.png"

# A tap reaches the host's input, which hands it to the engine.
read -r w h < <("$adb" -s $serial shell wm size | sed -n 's/.*: \([0-9]*\)x\([0-9]*\).*/\1 \2/p' | tail -1)
close_dialogs
"$adb" -s $serial shell input tap $((w / 2)) $((h / 2))
until_ok 'logged "touch start" && logged "touch end"' 30 "the tap"
say "tap"

# Home drops the surface (the host waits for the engine to let go) and pauses; back again gives a
# new surface and resumes.
# Counted, since a dialog over the game also pauses and resumes it.
released=$(count "surface released") paused=$(count "paused") resumed=$(count "resumed")
"$adb" -s $serial shell input keyevent KEYCODE_HOME
until_ok '(( $(count "surface released") > released && $(count "paused") > paused ))' 30 "the surface released"
say "$(log | grep -o 'surface released in .*' | tail -1)"
"$adb" -s $serial shell am start -W -n $pkg/.SmokeActivity >/dev/null
until_ok '(( $(count "resumed") > resumed ))' 30 "resumed"
sleep "$settle"
close_dialogs
"$adb" -s $serial exec-out screencap -p > "$results/$game-after-replace.png"
say "resumed on a new surface"

# Quit: the engine stops and its last event is exited.
"$adb" -s $serial shell am start -n $pkg/.SmokeActivity --ez quit true >/dev/null
until_ok 'logged "\"event\":\"exited\"" && logged "closed"' 60 "exited"
say "$(log | grep -o 'event {"event":"exited".*' | tail -1)"
app_log > "$results/$game-logcat.txt"

size=$(wc -c < "$so")
gz=$(gzip -9c "$so" | wc -c)
apk_size=$(wc -c < "$apk")
first=$(log | grep -o 'first frame in [0-9]* ms' | head -1 | grep -o '[0-9]*')
{
  echo "# Android smoke: $game"
  echo
  echo "$(date -u +%Y-%m-%dT%H:%MZ), emulator API 34 arm64 (-gpu ${SMOKE_GPU:-swiftshader_indirect}), bevy-ts $(log | grep -o 'version [^ ]*' | head -1 | cut -d' ' -f2)."
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
