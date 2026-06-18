# Daimon for Unreal Engine (C++)

Embed deterministic, drive-driven autonomous minds in your UE5 game. This is the
Unreal counterpart to `integrations/unity/` — both wrap the same native engine
(the `daimon-ffi` crate) over a small, flat-JSON C ABI.

`DaimonCore` is a self-contained UE **module** that depends only on engine
modules that ship with Unreal (`Core`, `CoreUObject`, `Engine`, `Json`) — **no
external package manager, no plugin marketplace deps**. It exposes a C++ RAII
wrapper (`FDaimonAgent`) and an optional Blueprint component
(`UDaimonAgentComponent`).

> **Honest scope.** Daimon is **not an LLM**. There is no vision, no language
> model, no internet. It is a compact, deterministic cognitive engine over a
> **grid world**: drives (hunger/thirst/safety/curiosity/social) → goals →
> actions, with a short inner monologue for flavour and debugging. You feed it
> what an NPC senses as integer grid coordinates + a few scalars; it returns one
> action per tick. It is small, fast, fully offline, and reproducible.

---

## What you get

| File | Role |
|---|---|
| `DaimonCore/DaimonCore.Build.cs` | UE module build rules; links the native lib (dynamic). |
| `DaimonCore/Public/DaimonCoreModule.h` / `Private/DaimonCoreModule.cpp` | Module impl; loads/frees the shared lib. |
| `DaimonCore/Public/DaimonAgent.h` / `Private/DaimonAgent.cpp` | `FDaimonAgent` RAII wrapper + flat `USTRUCT`s + JSON build/parse. |
| `DaimonCore/Public/DaimonAgentComponent.h` / `Private/.cpp` | `UDaimonAgentComponent` for Blueprints. |
| `Example/DaimonNpcActor.{h,cpp}` | Copy-paste example: world → perception → think → action. |
| `ThirdParty/Daimon/{include,lib/<platform>}` | Where you drop `daimon.h` + the built native lib. |

---

## Install

### 1. Build the native library

From the repo root:

```sh
cargo build -p daimon-ffi --release
```

This produces (under `target/release/`) BOTH the dynamic and static artifacts:

| Platform | Dynamic (used here) | Import lib (Windows) | Static (alternative) |
|---|---|---|---|
| macOS | `libdaimon_ffi.dylib` | — | `libdaimon_ffi.a` |
| Linux | `libdaimon_ffi.so` | — | `libdaimon_ffi.a` |
| Windows | `daimon_ffi.dll` | `daimon_ffi.dll.lib` | `daimon_ffi.lib` |

> On Windows the import lib that accompanies the cdylib may be named
> `daimon_ffi.dll.lib` or `daimon_ffi.lib` depending on toolchain — the
> `.Build.cs` expects `daimon_ffi.dll.lib`; rename if needed.

### 2. Drop the artifacts into ThirdParty

```
integrations/unreal/ThirdParty/Daimon/
  include/daimon.h                         <- copy from crates/daimon-ffi/include/
  lib/Mac/libdaimon_ffi.dylib
  lib/Linux/libdaimon_ffi.so
  lib/Win64/daimon_ffi.dll
  lib/Win64/daimon_ffi.dll.lib             <- the import lib
```

(`daimon.h` is the contract this module wraps; `DaimonAgent.cpp` also re-declares
the eight functions locally, so the build does not strictly require the header,
but you should keep it for reference and IDE navigation.)

### 3. Add the module to your project

Copy the `DaimonCore/` folder into your project's `Source/` (or into a plugin's
`Source/`). Then:

**`<Project>.uproject`** — add the module:

```json
"Modules": [
  { "Name": "YourGame",   "Type": "Runtime", "LoadingPhase": "Default" },
  { "Name": "DaimonCore", "Type": "Runtime", "LoadingPhase": "Default" }
]
```

**`Source/<Project>.Target.cs`** and **`<Project>Editor.Target.cs`** — ensure the
module is built (usually automatic once it's in `Source/`; if you reference its
types from another module, add `"DaimonCore"` to that module's
`PublicDependencyModuleNames` in its own `.Build.cs`).

Regenerate project files (right-click the `.uproject` → *Generate … project
files*, or `GenerateProjectFiles`), then build.

### 4. Use it

**C++:**

```cpp
#include "DaimonAgent.h"

TUniquePtr<FDaimonAgent> Mind =
    FDaimonAgent::New(TEXT("{\"name\":\"Mara\",\"curiosity\":0.9}"), /*Seed=*/42);

FDaimonPerception P;
P.Body.X = 5; P.Body.Y = 5; P.Body.Energy = 0.3f;
FDaimonEntity Berry; Berry.Id = 2; Berry.Kind = TEXT("food");
Berry.X = 6; Berry.Y = 5; Berry.Label = TEXT("berry");
P.Visible.Add(Berry);

FDaimonThought T = Mind->Think(P);   // T.Action, T.Dir, T.Target, T.Inner, …
// Mind frees the native handle automatically when it goes out of scope.
```

**Blueprint:** add a *Daimon Agent* component to your NPC actor, set
`PersonaJson` + a distinct `Seed`, then call `Think` each tick and branch on the
returned `Action`. See `Example/DaimonNpcActor.cpp`.

---

## Packaging (shipping the runtime lib)

This module links the native lib **dynamically**, so the shared library must
travel with your packaged game.

- The `.Build.cs` registers the lib as a **`RuntimeDependency`** copied to
  `$(BinaryOutputDir)` (i.e. next to the executable, under `Binaries/<Platform>/`).
  UAT copies runtime dependencies into the staged/packaged build automatically.
- On **Windows** the DLL is **delay-loaded**; `DaimonCoreModule::StartupModule`
  calls `FPlatformProcess::GetDllHandle` to resolve it explicitly at module load.
- Verify after packaging that `daimon_ffi.dll` / `libdaimon_ffi.dylib` /
  `libdaimon_ffi.so` sits beside the packaged executable. If `GetDllHandle`
  logs a load failure at startup, the lib was not staged — check that the file
  is present in `ThirdParty/Daimon/lib/<Platform>/` before packaging.

**Static alternative** (single self-contained binary; required for iOS): see the
commented block at the bottom of `DaimonCore.Build.cs`. Drop the `.a`/`.lib`
instead, remove the dynamic block, and define `DAIMON_STATIC`.

---

## The two mappings (your only real work)

**1. World → perception** (`FDaimonPerception`):

- `Body` — integer grid `X/Y` (required), plus `Health/Energy/Hydration` (0..1),
  and optionally `Enclosure`, `Season`, `WinterIn`, `Carrying`, and the three
  direction hints (`ShelterGap`, `GatherDir`, `StoreDir`; leave empty for null).
- `Visible[]` — one entity per perceived thing. `Kind` ∈
  `food | water | agent | predator | curio`; `Id` is a stable per-entity id.
- `Events[]` — what happened *to* this NPC this tick. `Kind` ∈
  `ate | drank | hurt | repelled | heard | discovered | vanished | died | told`
  (the complete inbound set); set only the fields the kind needs (e.g. `hurt` →
  `Id`, `Health`; `heard` → `From`, `Text`; `died` → `Id`, `X`, `Y`, `Cause`).
  `told` is inter-agent info sharing the listener can act on (vs sentiment-only
  `heard`): `Info` ∈ `greeting|resource_at|danger_at`, plus `Id`/`EntityKind`/
  `X`/`Y`/`Label` for `resource_at`. Unknown kinds are ignored.

**2. Action → world** (`FDaimonThought`):

| `Action` | Read | Meaning |
|---|---|---|
| `move` | `Dir` (`north/south/east/west`) | step one cell |
| `eat`/`drink`/`inspect`/`strike` | `Target` (entity id, `bHasTarget`) | interact with an entity |
| `talk` | `Target` + `Text` | speak to an entity |
| `build` | `PosX`,`PosY` (`bHasPos`) | place a structure |
| `gather`/`store`/`rest`/`wait` | — | act in place |

`Goal`, `Drive`, `Process` (`reflex/routine/deliberate`) and `Inner` are
informational — great for a debug overlay.

---

## Determinism

Same seed + same perception stream ⇒ same behaviour (within one build/platform).

- **Distinct per-NPC seed.** Two agents that share a seed behave identically the
  moment they share a percept. Hash a *stable* identity (e.g. the actor's
  `FName`/GUID), not `FMath::Rand()`, so the seed survives a restart.
- **Fixed tick order.** Tick agents in a deterministic order each frame.
- **⚠ UE parallel ticking caveat.** Unreal can tick actors/components across
  worker threads and in non-deterministic order. For reproducible runs, drive
  `Think` from a **single, ordered** place (e.g. one manager actor that iterates
  a sorted array of minds), and tick a given `FDaimonAgent` from one thread only
  (the wrapper is not internally synchronised). The native engine's error path
  is panic-safe (a fault returns a `wait` thought, never a crash across the FFI
  boundary).
- Cross-platform bit-identical floating point is **not** guaranteed.

---

## Verify before you trust it

- **UE version:** written against UE **5.3+** conventions (`StringCast<UTF8CHAR>`,
  `Json` module, `$(BinaryOutputDir)`). Confirm against your exact engine version.
- **Platform:** the `.Build.cs` handles `Win64`, `Mac`, `Linux`. iOS/Android need
  the static-link path (see the Build.cs comment).
- This module was authored without an Unreal toolchain to compile against — treat
  the first `GenerateProjectFiles` + build as the real check.
