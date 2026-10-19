// Type tests for `app.level` (levels): `tsc -p test` must pass, and every `@ts-expect-error` must
// be an error.
import { app } from "bevy-ts";

app.level("levels/arena.bsn");
// @ts-expect-error a level is a .bsn file
app.level("levels/arena.json");
