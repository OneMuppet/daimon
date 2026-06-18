# AGENTS.md — Daimon Unity integration (for coding agents)

Native engine: pure Rust, exposed as a C ABI by the `daimon-ffi` crate. **Do not
modify any crate.** This package is C# glue only.

## Native artifact & binding

- Build: `cargo build -p daimon-ffi --release`.
- Stem `daimon_ffi`: `libdaimon_ffi.dylib` (macOS), `libdaimon_ffi.so` (Linux),
  `daimon_ffi.dll` (Windows), `libdaimon_ffi.a` (iOS static).
- Binding: `[DllImport("daimon_ffi", CallingConvention = CallingConvention.Cdecl)]`.
  iOS branch: `#if UNITY_IOS && !UNITY_EDITOR` ⇒ `DllImport("__Internal")` with the
  static `.a` linked into the app. `[DllImport]` is fine under Mono **and** IL2CPP.
- All strings are **UTF-8**. Calling convention is **Cdecl** for every function.

## The 8 functions (exact C signatures)

```c
DaimonAgent* daimon_agent_new(const char* persona_json, uint64_t seed);
char*        daimon_agent_think(DaimonAgent*, const char* input_json);
char*        daimon_agent_save(DaimonAgent*);
DaimonAgent* daimon_agent_load(const char* name, const char* mind_json);
void         daimon_agent_free(DaimonAgent*);
void         daimon_string_free(char*);
const char*  daimon_version(void);   // STATIC — never free
char*        daimon_last_error(void);// free after reading; reading clears it
```

C# mapping (see `Runtime/DaimonNative.cs`):
- `const char*` IN  ⇒ `[MarshalAs(UnmanagedType.LPUTF8Str)] string`.
- `char*` / `const char*` OUT ⇒ **`IntPtr`** (never `string` — auto-marshal would
  free with the wrong allocator and could leak/corrupt).
- `DaimonAgent*` ⇒ `IntPtr`. `uint64_t` ⇒ `ulong`.

## Memory rules (THE gotchas — enforce exactly)

1. Every non-null `char*` RETURNED by `think` / `save` / `last_error` is owned by
   the caller: copy it to a managed string via `Marshal.PtrToStringUTF8(ptr)`
   (fallback: manual byte-copy under `DAIMON_NO_UTF8_MARSHAL`), then **always**
   call `daimon_string_free(ptr)`. The wrapper's `TakeString` does this in a
   `finally`.
2. `daimon_version()` returns a **static** string — **never** free it. It is the
   one OUT pointer that must NOT go through `TakeString`/`daimon_string_free`.
3. Every handle from `new`/`load` must be freed with `daimon_agent_free` exactly
   once. `DaimonAgent` does this in `Dispose()` (call from `OnDestroy`) with a
   finalizer fallback; the free is guarded by `Interlocked.Exchange` so a
   Dispose/finalize race frees only once. The native side is NULL-safe.
4. On a null return, call `daimon_last_error()` for a diagnostic. The wrapper
   throws `DaimonException` with that message.

## JSON contract (FLAT — plain scalars, no tagged unions)

**Persona in** (all optional):
`{"name","boldness","sociability","curiosity","creed"}` — floats 0..1.

**Perception in:**
```json
{ "body": {"x","y","health","energy","hydration","enclosure","season",
           "winter_in","carrying","shelter_gap","gather_dir","store_dir"},
  "visible": [ {"id","kind","x","y","label"} ],
  "events":  [ {"kind","id","from","energy","health","x","y","text","cause"} ] }
```
- `visible.kind` ∈ `food|water|agent|predator|curio`.
- `event.kind` ∈ `ate|drank|hurt|repelled|heard|discovered|vanished|died|told`
  (complete inbound set; each reads only the fields it needs; unknown kinds are
  ignored, not fatal). `told` = inter-agent info sharing the listener acts on
  (vs sentiment-only `heard`): use `WorldEventDto.ToldResource/ToldDanger/
  ToldGreeting`; `info` ∈ `greeting|resource_at|danger_at`.
- directions ∈ `north|south|east|west|null`. `winter_in` defaults to a huge value
  (`float.MaxValue`) meaning "never". serde rejects NaN/Inf — the builder maps
  non-finite floats to finite, NaN→0.

**Decision out:**
```json
{ "action","dir","target","pos","text","goal","drive","process","inner" }
```
- `action` ∈ `move|eat|drink|talk|inspect|strike|build|gather|store|rest|wait`.
- `move`⇒read `dir`; `eat|drink|inspect|strike|talk`⇒read `target` (entity id);
  `build`⇒read `pos` (`[x,y]`); `talk`⇒also `text`. `process` ∈
  `reflex|routine|deliberate`.

## Determinism

Distinct seed per NPC; tick in a fixed order for reproducibility. Same seed + same
perception stream ⇒ same behaviour within one build/platform. No cross-platform FP
guarantee.

## Dependencies

None beyond `UnityEngine` + BCL. No Newtonsoft, no `System.Text.Json`. JSON is
built with `StringBuilder` and read with the small flat parser in `Json`
(top-level scalars + the `pos` array only). Newtonsoft is an OPTIONAL drop-in
replacement, not a dependency.

## Files

- `Runtime/DaimonNative.cs` — 8 externs, Cdecl, LPUTF8Str in / IntPtr out, iOS
  `__Internal` branch.
- `Runtime/DaimonAgent.cs` — `DaimonAgent : IDisposable`, `Perception`/`Body`/
  `VisibleEntity`/`WorldEventDto` (+`ToJson`), `Thought` (`Parse`), `Json` helper,
  `Utf8` decode helper, `DaimonException`.
- `Samples~/DaimonNpc.cs` — example MonoBehaviour (not compiled until imported).
- `README.md`, `package.json`.

## Scope (be honest in any user-facing copy)

Not an LLM. No vision, no network. Flat grid-world perception, deterministic.
