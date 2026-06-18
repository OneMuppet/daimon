# Daimon for Unity

Embed **Daimon** — a deterministic, drive-driven cognitive engine — in your Unity
NPCs. Spawn one mind per NPC; each tick you hand it a small JSON description of
what the NPC senses and it hands back a small JSON description of what it decided.
You implement two mappings: *your world → perception* and *decision → effects in
your world*.

> **Honest scope.** Daimon is **not** an LLM. There is no language model, no
> vision, no network call. It is a compact, fully deterministic cognitive engine
> operating over a **flat grid-world** perception (integer cell coordinates,
> survival drives, a short list of visible entities and recent events). It runs
> entirely in-process as native code, so it is fast and offline. What it gives you
> is believable, legible, reproducible NPC behaviour — `goal`, `drive`, `process`,
> and an inner-monologue line per tick — not open-ended conversation.

---

## 1. Build the native library

From the repo root (this does **not** modify any crate):

```sh
cargo build -p daimon-ffi --release
```

This produces, in `target/release/`, depending on your host:

| Platform | Built artifact            | Goes into (Unity project)                  |
|----------|---------------------------|--------------------------------------------|
| macOS    | `libdaimon_ffi.dylib`     | `Assets/Plugins/macOS/`                    |
| Linux    | `libdaimon_ffi.so`        | `Assets/Plugins/Linux/x86_64/`             |
| Windows  | `daimon_ffi.dll`          | `Assets/Plugins/x86_64/`                   |
| iOS      | `libdaimon_ffi.a` (static)| `Assets/Plugins/iOS/` (see iOS note below) |

Cross-compiling for a target platform requires the matching Rust target
(`rustup target add aarch64-apple-ios`, etc.) and building with
`--target <triple>`; the artifact then lands in `target/<triple>/release/`.

### DllImport name

The C# bindings use **`DllImport("daimon_ffi")`** with **`CallingConvention.Cdecl`**.
The file *stem* is `daimon_ffi`, so the OS loader resolves it for every desktop
platform:

- macOS / Linux: the loader prepends `lib`, matching `libdaimon_ffi.{dylib,so}`.
- Windows: appends `.dll`, matching `daimon_ffi.dll`.

So **one** import name works across macOS, Linux, and Windows. Place the binary in
the platform plugin folder above and set its platform/CPU in the Unity plugin
inspector.

### iOS (and other static-link / IL2CPP-AOT cases)

iOS forbids loading dynamic libraries by name, so there the engine must be a
**static** library (`libdaimon_ffi.a`) **linked into the app binary**, and the
import name must be `"__Internal"`. `Runtime/DaimonNative.cs` already branches:

```csharp
#if UNITY_IOS && !UNITY_EDITOR
    private const string Lib = "__Internal";
#else
    private const string Lib = "daimon_ffi";
#endif
```

Drop `libdaimon_ffi.a` into `Assets/Plugins/iOS/` and Unity links it into the
Xcode project automatically. `[DllImport]` works under **both Mono and IL2CPP** —
it is native code either way; IL2CPP just AOT-compiles the managed side. On
desktop IL2CPP builds you still ship the dynamic library and use `"daimon_ffi"`.

---

## 2. Install the C# package

Copy this folder into your Unity project (e.g. `Assets/Daimon/`), **or** add it as
a local UPM package via `Packages/manifest.json`:

```json
{ "dependencies": { "io.ampdlabs.daimon": "file:../path/to/integrations/unity" } }
```

Files:

- `Runtime/DaimonNative.cs` — raw P/Invoke bindings (internal).
- `Runtime/DaimonAgent.cs` — the `DaimonAgent` wrapper + `Perception` / `Thought`
  DTOs + a tiny zero-dependency JSON builder/parser.
- `Samples~/DaimonNpc.cs` — example MonoBehaviour (import via Package Manager, or
  copy out of `Samples~`).

### Unity version / marshalling

Recommended: **Unity 2021.2+** with API Compatibility Level **.NET Standard 2.1**
(the modern default). That gives `Marshal.PtrToStringUTF8` and
`UnmanagedType.LPUTF8Str`, which the bindings use for correct UTF-8 marshalling.

On older Unity (.NET Standard 2.0): `UnmanagedType.LPUTF8Str` may not marshal
inbound strings correctly, and `Marshal.PtrToStringUTF8` is absent. Define the
scripting symbol **`DAIMON_NO_UTF8_MARSHAL`** to switch returned-string decoding to
a manual byte-copy fallback (always correct). The *inbound* `LPUTF8Str` path is
not papered over — if you must target .NET Standard 2.0, verify your strings are
ASCII or upgrade the API level. *(Unverified on 2.0 here — see "What was not
compile-checked" at the bottom.)*

### Zero external dependencies

This package intentionally depends on **nothing** beyond `UnityEngine` and the BCL
— no Newtonsoft, no `System.Text.Json` (older Unity lacks the latter). The flat
perception JSON is built with a `StringBuilder`; the flat decision JSON is read by
a small tolerant parser in `Json` (it relies on the decision being a *flat* object
plus the one `pos` array). 

**Optional, cleaner:** if your project already ships Newtonsoft
(`com.unity.nuget.newtonsoft-json`), you can replace `Perception.ToJson()` and
`Thought.Parse()` with `JsonConvert` calls against `[JsonProperty]`-annotated DTOs.
The native contract is unchanged. We do **not** depend on it so the package drops
into any Unity version.

---

## 3. The two mappings

**Your world → perception** (build a `Perception`, call `Think`):

| Perception field        | Meaning                                                        |
|-------------------------|----------------------------------------------------------------|
| `Body.X`, `Body.Y`      | NPC grid cell (integers)                                       |
| `Body.Health/Energy/Hydration` | survival stats, 0..1 (default 1.0)                      |
| `Body.Enclosure`        | how walled-in the NPC is, 0..1                                 |
| `Body.Carrying`         | carried resource, 0..1                                         |
| `Body.Season`           | 0..3                                                           |
| `Body.WinterIn`         | ticks until winter; `float.MaxValue` ≈ "never"                 |
| `Body.ShelterGap/GatherDir/StoreDir` | a direction or `null`                             |
| `Visible[]`             | `id, kind(food/water/agent/predator/curio), x, y, label`       |
| `Events[]`              | things that happened to this NPC this tick (see `WorldEventDto`)|

**Decision → effects** (read the returned `Thought`):

| `Thought.Action` | Read these fields                          |
|------------------|--------------------------------------------|
| `move`           | `Dir` (north/south/east/west)              |
| `eat`/`drink`/`inspect`/`strike` | `Target` (entity id)       |
| `talk`           | `Target` (id) + `Text` (utterance)         |
| `build`          | `Pos` (`int[2]` = `[x, y]`)                |
| `gather`/`store`/`rest`/`wait` | (no extra fields)            |

Every `Thought` also carries `Goal`, `Drive`, `Process`
(`reflex`/`routine`/`deliberate`), and `Inner` (an inner-monologue line) — great
for debug HUDs and barks.

---

## 4. Determinism rules

- **Distinct seed per NPC.** Two agents sharing a seed behave identically the
  moment they share a percept. The sample derives a seed from the instance id when
  you leave `seed = 0`.
- **Fixed tick order.** For reproducible runs, tick agents in a stable order (e.g.
  sorted by a persistent id) on a fixed-step clock.
- **Same seed + same perception stream ⇒ same behaviour**, within one build and
  platform. Cross-platform bit-identical floating point is *not* guaranteed.

---

## 5. Memory & UTF-8 (handled for you)

The wrapper enforces the engine's one rule so you never touch a pointer:

- Strings passed **in** are marshalled as UTF-8 (`LPUTF8Str`) and copied by the
  engine — you keep ownership.
- Strings returned **out** (`think`, `save`, `last_error`) are received as
  `IntPtr`, copied to a managed string, then freed with `daimon_string_free`.
- `daimon_version()` returns a **static** string and is the one pointer that is
  **never** freed.
- Every `DaimonAgent` owns its native handle and frees it in `Dispose()` (call it
  in `OnDestroy`); a finalizer is the safety net.

---

## 6. Copy-paste usage

```csharp
using Daimon;
using UnityEngine;

public class Quickstart : MonoBehaviour
{
    private DaimonAgent _agent;

    void Start()
    {
        Debug.Log("Daimon engine v" + DaimonAgent.Version());

        // Distinct seed per NPC!
        _agent = new DaimonAgent(
            name: "Mara",
            boldness: 0.6f, sociability: 0.4f, curiosity: 0.9f,
            creed: "I want to understand this place.",
            seed: 42);
    }

    void Tick()
    {
        var p = new Perception(Body.Default(5, 5));
        p.Body.Energy = 0.3f;
        p.Visible.Add(new VisibleEntity(2, "food", 6, 5, "berry"));
        p.Events.Add(WorldEventDto.Hurt(id: 99, health: 0.2f));

        Thought t = _agent.Think(p);
        Debug.Log($"{t.Action} ({t.Drive}/{t.Goal}): {t.Inner}");

        if (t.Action == "move")        MoveNpc(t.Dir);
        else if (t.Action == "eat")    EatEntity(t.Target);
        else if (t.Action == "build")  BuildAt(t.Pos);   // t.Pos = [x, y]
        // … see Samples~/DaimonNpc.cs for the full switch
    }

    void OnDestroy() => _agent?.Dispose();

    void MoveNpc(string dir) { /* your game */ }
    void EatEntity(uint? id) { /* your game */ }
    void BuildAt(int[] pos)  { /* your game */ }
}
```

Save / load a mind for save-games:

```csharp
string mindJson = _agent.Save();           // persist this
// … later …
var restored = DaimonAgent.Load("Mara", mindJson);
```
