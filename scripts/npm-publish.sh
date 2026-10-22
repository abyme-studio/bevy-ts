#!/usr/bin/env bash
# Publishes one package directory to npm, surviving npm's transient errors.
#   scripts/npm-publish.sh <dir> <name> <version> [npm publish flags...]
# A version already on npm is success, so a rerun is idempotent. A 5xx or a 404 (npm answers a
# publish with 404 for a moment after an outage, and for a package with no trusted publisher) is
# retried with backoff; anything else fails at once. Every attempt is a fresh `npm publish`, so a
# fresh OIDC exchange: a long run never publishes on an expired token.
set -uo pipefail
dir=$1 name=$2 version=$3; shift 3
source "$(dirname "$0")/npm-lib.sh"

case "$(on_npm "$name" "$version")" in
  yes) echo "$name@$version is already on npm; skipped"; exit 0 ;;
  error) echo "::error::cannot tell whether $name@$version is on npm" >&2; exit 1 ;;
esac

delay=30
for attempt in 1 2 3 4 5; do
  log=$(mktemp)
  (cd "$dir" && npm publish "$@") 2>&1 | tee "$log"
  status=${PIPESTATUS[0]}
  [ "$status" -eq 0 ] && exit 0
  if grep -qiE 'previously published|cannot publish over' "$log"; then
    echo "$name@$version is already on npm; taken as published"; exit 0
  fi
  if ! grep -qE 'E5[0-9][0-9]|E404|ETIMEDOUT|ECONNRESET|EAI_AGAIN| 5[0-9][0-9] ' "$log"; then
    echo "::error::$name@$version did not publish (not a transient npm error)" >&2; exit 1
  fi
  # A 5xx can come after the registry took the package; look before trying again.
  [ "$(on_npm "$name" "$version")" = yes ] && { echo "$name@$version is on npm after all"; exit 0; }
  [ "$attempt" -eq 5 ] && break
  echo "::warning::$name@$version attempt $attempt failed transiently; retrying in ${delay}s"
  sleep "$delay"; delay=$((delay * 2))
done
echo "::error::$name@$version did not publish after 5 attempts" >&2
exit 1
