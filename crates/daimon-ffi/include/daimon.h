/*
 * daimon.h — C ABI for the Daimon cognitive engine.
 *
 * Embed deterministic, drive-driven autonomous minds in any C / C++ / C# host
 * (Unity, Unreal, …). The engine is pure Rust; link the native library built by
 * the `daimon-ffi` crate (libdaimon_ffi.{dylib,so} or .dll / .a / .lib).
 *
 * THE CONTRACT
 *   Spawn one agent per NPC (daimon_agent_new). Each tick, pass a flat JSON
 *   object describing what the NPC senses (daimon_agent_think); you get back a
 *   flat JSON object describing what it decided. You do two mappings: your world
 *   -> the perception JSON, and the returned action -> effects in your world.
 *
 * JSON IS FLAT (plain scalar fields, no tagged unions) so any JSON library can
 * read/write it. See the daimon-ffi crate docs for the exact schemas, or:
 *
 *   perception in : { "body": {"x","y","health","energy","hydration", ...},
 *                     "visible": [ {"id","kind","x","y","label"} ],
 *                     "events":  [ {"kind","id", ...} ] }
 *
 * Inbound event kinds (the complete set):
 *   ate|drank|hurt|repelled|heard|discovered|vanished|died|told
 * `told` is inter-agent info sharing (acts on content, unlike sentiment-only
 * `heard`): {"kind":"told","from":N,"info":"resource_at","id":N,
 * "entity_kind":"food","x":N,"y":N,"label":"…"} | {"...","info":"danger_at",
 * "x":N,"y":N} | {"...","info":"greeting"}.
 *   decision  out : { "action","dir","target","pos","text",
 *                     "goal","drive","process","inner" }
 *
 * MEMORY OWNERSHIP (the one rule)
 *   - Every non-NULL `char*` RETURNED by this library is owned by you and must be
 *     released with daimon_string_free().  (Exception: daimon_version() returns a
 *     static string — do NOT free it.)
 *   - Every DaimonAgent* must be released with daimon_agent_free().
 *   - Strings you PASS IN are borrowed and copied; you keep ownership.
 *   - On error, functions return NULL; call daimon_last_error() for a message.
 *   - All calls are panic-safe: a fault becomes a NULL return, never a crash
 *     across the boundary.
 *
 * DETERMINISM
 *   Same seed + same perception stream => same behaviour (within one build/
 *   platform). Give each NPC a DISTINCT seed and tick agents in a fixed order.
 *   Cross-platform bit-identical floating point is not guaranteed.
 */
#ifndef DAIMON_H
#define DAIMON_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* Opaque handle to one agent's mind. */
typedef struct DaimonAgent DaimonAgent;

/* Spawn an agent from a persona JSON (all fields optional) + a deterministic
 * seed. Returns NULL on error (see daimon_last_error). Free with
 * daimon_agent_free. */
DaimonAgent *daimon_agent_new(const char *persona_json, uint64_t seed);

/* Advance the mind one tick. `input_json` is the flat perception object.
 * Returns a newly-allocated decision JSON string (free with daimon_string_free),
 * or NULL on error. */
char *daimon_agent_think(DaimonAgent *agent, const char *input_json);

/* Serialise the mind to a JSON save string (free with daimon_string_free). */
char *daimon_agent_save(DaimonAgent *agent);

/* Restore a mind from a daimon_agent_save string. `name` labels the agent.
 * Returns a handle (free with daimon_agent_free) or NULL. */
DaimonAgent *daimon_agent_load(const char *name, const char *mind_json);

/* Destroy an agent handle. NULL-safe. */
void daimon_agent_free(DaimonAgent *agent);

/* Free a string previously returned by this library. NULL-safe. */
void daimon_string_free(char *s);

/* Library version. Static — do NOT free. */
const char *daimon_version(void);

/* Last error on this thread (free with daimon_string_free), or NULL if none.
 * Reading it clears the stored error. */
char *daimon_last_error(void);

#ifdef __cplusplus
} /* extern "C" */
#endif

#endif /* DAIMON_H */
