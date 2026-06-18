# AGENTS.md — DaimonCore (Unreal) for coding agents

Terse, precise contract for editing this UE module. The source of truth is
`/crates/daimon-ffi/include/daimon.h` and `/crates/daimon-ffi/src/lib.rs`. Do
NOT modify any crate.

## The C ABI (Cdecl, UTF-8, opaque `DaimonAgent*`)

```c
DaimonAgent* daimon_agent_new(const char* persona_json, uint64_t seed);
char*        daimon_agent_think(DaimonAgent*, const char* input_json); // free
char*        daimon_agent_save(DaimonAgent*);                          // free
DaimonAgent* daimon_agent_load(const char* name, const char* mind_json);
void         daimon_agent_free(DaimonAgent*);        // NULL-safe
void         daimon_string_free(char*);              // NULL-safe
const char*  daimon_version(void);  // STATIC — never free
char*        daimon_last_error(void);                // free; reading clears it
```

`DaimonAgent.cpp` re-declares these in an `extern "C"` block (works for both
static and dynamic linkage). On error a function returns NULL and sets
`daimon_last_error()`.

## Memory rules (non-negotiable)

- Every non-null `char*` RETURNED must be freed with `daimon_string_free` after
  copying to FString. The helper `TakeOwnedCString(char*)` does copy-then-free in
  one place — use it for `think`/`save`/`last_error`.
- `daimon_version()` is STATIC — copy it, NEVER free it (`FDaimonAgent::Version`
  does this correctly; do not route it through `TakeOwnedCString`).
- Strings passed IN are borrowed/copied — caller keeps ownership.
- The opaque `DaimonAgent*` is freed in `~FDaimonAgent`; `FDaimonAgent` is
  move-only (no copy), and `UDaimonAgentComponent` holds it in a `TUniquePtr`.

## FString ↔ UTF-8 (the gotcha)

- **Inbound** (FString → `const char*`): `StringCast<UTF8CHAR>(*Str)`, keep the
  conversion object alive on the stack, pass
  `reinterpret_cast<const char*>(conv.Get())`. (`TCHAR_TO_UTF8` also works but
  must not be stored — it's a temporary.) Daimon copies it, so the conversion
  only needs to outlive the call.
- **Outbound** (`char*` → FString): `UTF8_TO_TCHAR(ptr)` into an FString, THEN
  `daimon_string_free(ptr)`.

## JSON — Unreal's BUILT-IN `Json` module only (no external deps)

Build the FLAT perception object and parse the FLAT decision object with
`FJsonObject` / `FJsonSerializer`. Flat = plain scalars, no tagged unions.

- Build: `MakeShared<FJsonObject>()`, `SetNumberField`/`SetStringField`,
  arrays as `TArray<TSharedPtr<FJsonValue>>` of `MakeShared<FJsonValueObject>`,
  null via `SetField(Key, MakeShared<FJsonValueNull>())`. Serialize with
  `TJsonWriterFactory<>::Create(&Out)` + `FJsonSerializer::Serialize`.
- Parse: `TJsonReaderFactory<>::Create(Json)` + `FJsonSerializer::Deserialize`,
  then `TryGetStringField` / `TryGetNumberField`; gate optional fields with
  `HasTypedField<EJson::String>` / `HasTypedField<EJson::Number>`; read `pos`
  with `TryGetArrayField`.

### Wire shapes (must match serde DTOs in daimon-ffi/src/lib.rs)

- **persona in**: `{"name","boldness","sociability","curiosity","creed"}` — all
  optional.
- **perception in**: `{"body":{"x","y","health","energy","hydration","enclosure",
  "season","winter_in","carrying","shelter_gap","gather_dir","store_dir"},
  "visible":[{"id","kind","x","y","label"}],"events":[{"kind","id","from",
  "energy","health","x","y","text","cause"}]}`.
  - entity `kind`: `food|water|agent|predator|curio`
  - event `kind`: `ate|drank|hurt|repelled|heard|discovered|vanished|died|told` (complete inbound set; `told` = inter-agent info sharing the listener acts on, vs sentiment-only `heard`: set `Info` = `greeting|resource_at|danger_at`, plus `Id`/`EntityKind`/`X`/`Y`/`Label` for `resource_at`)
  - dirs: `north|south|east|west` or `null` (empty FString → null)
- **decision out**: `{"action","dir","target","pos","text","goal","drive",
  "process","inner"}`.
  - `action`: `move|eat|drink|talk|inspect|strike|build|gather|store|rest|wait`
  - `move`→`dir`; `eat|drink|inspect|strike|talk`→`target`; `build`→`pos` `[x,y]`;
    `talk`→`target`+`text`.

Type notes: ids are `uint32` on the wire; held as `int64` in USTRUCTs (Blueprint
has no `uint32`) and cast back to `double` when written to JSON. `winter_in`
default ≈ `f32::MAX` (3.4e38) meaning "never".

## Linkage (`.Build.cs`)

- Engine deps only: `Core`, `CoreUObject`, `Engine`, `Json`.
- `PublicIncludePaths += ThirdParty/Daimon/include`.
- **DYNAMIC** (chosen): Win64 → `PublicAdditionalLibraries += daimon_ffi.dll.lib`,
  `PublicDelayLoadDLLs += "daimon_ffi.dll"`, `RuntimeDependencies.Add(
  "$(BinaryOutputDir)/daimon_ffi.dll", <src>)`. Mac/Linux →
  `PublicAdditionalLibraries += libdaimon_ffi.{dylib,so}` +
  `RuntimeDependencies`. `PublicDefinitions += DAIMON_DLL_NAME=...` so
  `DaimonCoreModule::StartupModule` can `FPlatformProcess::GetDllHandle` it.
- **STATIC** alt: link `.a`/`.lib`, define `DAIMON_STATIC`, drop the delay-load /
  runtime-dep / GetDllHandle. Required on iOS.
- `IMPLEMENT_MODULE(FDaimonCoreModule, DaimonCore)` lives in
  `DaimonCoreModule.cpp`.

## Invariants when editing

- Field names in `BuildPerceptionJson` / `ParseThoughtJson` must stay byte-exact
  vs the serde DTOs. If the crate adds a field, mirror it here; don't rename.
- Never free `daimon_version()`. Never store a `TCHAR_TO_UTF8` temporary.
- `Think` must never throw across FFI — on a NULL return it synthesises a `wait`
  thought with the error in `Inner`. Preserve that.
- Determinism: distinct per-NPC seed, single-threaded ordered ticking (UE may
  tick in parallel — drive `Think` from one ordered place).
