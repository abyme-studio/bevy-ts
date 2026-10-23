# shellcheck shell=bash
# Sourced first by every script that runs cargo: runs the script again inside one of the machine's
# check slots, named bevy-ts:<script> (docs/check-slots.md). A caller that already holds a slot
# (ABYME_GATE_HELD) and a machine without gate-slot (CI) run it as it is.
gate_slot=${BEVY_TS_GATE_SLOT:-$HOME/dev/abyme/scripts/gate-slot}
if [[ -z "${ABYME_GATE_HELD:-}" && -x "$gate_slot" ]]; then
  exec "$gate_slot" run "bevy-ts:$(basename "$0" .sh)" -- "$0" "$@"
fi
