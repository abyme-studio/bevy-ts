/* bevy-ts's embedding API (docs/android.md). One engine per process; it runs on a thread of its
 * own. Every call returns at once except bevy_ts_surface with a null window and bevy_ts_destroy,
 * which wait (2 s and 5 s at most). Strings the library returns are freed with
 * bevy_ts_free_string. */
#ifndef BEVY_TS_EMBED_H
#define BEVY_TS_EMBED_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* Raised on an incompatible change to these functions, the config keys or the event shapes. */
#define BEVY_TS_ABI 1

typedef struct BevyTsEngine BevyTsEngine;

/* One JSON event: started, log, ime or exited (last). Called on the engine's thread; json is
 * NUL-terminated and len bytes long. Do not call bevy_ts_destroy or bevy_ts_surface with a null
 * window from inside it. */
typedef void (*bevy_ts_event_fn)(void *user, const char *json, size_t len);

/* config_json: { "game": "<absolute folder holding game.json>", "env": { "NAME": "value" },
 *   "headersOrigin": "https://host:port", "originHeaders": { "authorization": "Bearer <key>" },
 *   "headless": false }. originHeaders go only to headersOrigin over TLS; no script reads them.
 * window: the platform's native window (Android ANativeWindow *), or NULL.
 * width, height: physical pixels; scale: pixels per point.
 * Returns NULL with *error set (free it) when the game cannot play here. */
BevyTsEngine *bevy_ts_create(const char *config_json, void *window, uint32_t width, uint32_t height, float scale,
                             bevy_ts_event_fn on_event, void *user, char **error);
/* A new window, the same at a new size, or NULL: returns once the old window is let go. */
void bevy_ts_surface(BevyTsEngine *engine, void *window, uint32_t width, uint32_t height, float scale);
void bevy_ts_pause(BevyTsEngine *engine);
void bevy_ts_resume(BevyTsEngine *engine);
/* One SURFACE.md ViewportInput event as JSON, x and y in points from the surface's top left. */
void bevy_ts_input(BevyTsEngine *engine, const char *json, size_t len);
/* Stops the engine, sends exited, joins its thread; engine is invalid after. */
void bevy_ts_destroy(BevyTsEngine *engine);
void bevy_ts_free_string(char *s);
uint32_t bevy_ts_abi(void);
const char *bevy_ts_version(void);

#ifdef __cplusplus
}
#endif

#endif
