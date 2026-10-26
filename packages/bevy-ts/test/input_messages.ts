// Type tests for docs/native.md's input messages example (openspec script-input-devices stage 3):
// `tsc -p test` must pass.
import type { MessageReader, MessageWriter, Res } from "bevy-ts";
import { GamepadEvent, GamepadRumbleRequest, MouseScrollPixelsPerLine, MouseWheel, PinchGesture } from "bevy-ts/bevy";

declare function zoom(by: number): void;

export function pinchZoom(pinch: MessageReader<PinchGesture>, wheel: MessageReader<MouseWheel>, perLine: Res<MouseScrollPixelsPerLine>) {
  pinch.each((p) => zoom(p));
  wheel.each((w) => zoom(w.unit === "Line" ? w.y * perLine.value : w.y));
}
export function buzzOnConnect(events: MessageReader<GamepadEvent>, rumble: MessageWriter<GamepadRumbleRequest>) {
  events.each((e) => {
    if ("Connection" in e && e.Connection.connection !== "Disconnected") rumble.write(GamepadRumbleRequest.of({
      Add: { gamepad: e.Connection.gamepad, duration: 0.5, intensity: { strong_motor: 1, weak_motor: 0.25 } },
    }));
  });
}
