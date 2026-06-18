// DaimonCore.Build.cs — UE module that links the Daimon cognitive engine.
//
// Build rules for the DaimonCore runtime module. It depends only on engine
// modules that ship with Unreal (Core, CoreUObject, Engine, Json) — NO external
// package manager — and links the native `daimon-ffi` library from a ThirdParty
// folder.
//
// LINKING APPROACH: DYNAMIC (delay-loaded shared library).
//   We link against the *dynamic* artifact (libdaimon_ffi.dylib / .so /
//   daimon_ffi.dll). On Windows we link the small import lib (daimon_ffi.lib that
//   accompanies the DLL) and DELAY-LOAD the DLL, then copy the DLL into the
//   target's Binaries via RuntimeDependencies so it is found at runtime and gets
//   packaged. On Mac/Linux we add the .dylib/.so as a RuntimeDependency and add
//   its directory to the loader path. Rationale: the engine is panic-safe and
//   self-contained, a shared lib avoids relinking the whole editor when the
//   engine changes, and it mirrors the Unity integration (which loads the same
//   .dylib/.so/.dll). For a fully static link, see the commented block below.
//
// ARTIFACT LAYOUT (drop the files you built with
//   `cargo build -p daimon-ffi --release`):
//     ThirdParty/Daimon/include/daimon.h
//     ThirdParty/Daimon/lib/Mac/libdaimon_ffi.dylib
//     ThirdParty/Daimon/lib/Linux/libdaimon_ffi.so
//     ThirdParty/Daimon/lib/Win64/daimon_ffi.dll
//     ThirdParty/Daimon/lib/Win64/daimon_ffi.dll.lib   (import lib for the DLL)
//
// Place this module folder at <Project>/Source/DaimonCore/ (or in a plugin's
// Source/), then add "DaimonCore" to your .uproject modules and to your
// *.Target.cs ExtraModuleNames as described in README.md.

using System.IO;
using UnrealBuildTool;

public class DaimonCore : ModuleRules
{
	public DaimonCore(ReadOnlyTargetRules Target) : base(Target)
	{
		PCHUsage = PCHUsageMode.UseExplicitOrSharedPCHs;

		// Engine-only dependencies. Json is the BUILT-IN UE JSON module; we use it
		// to build the flat perception object and parse the flat decision object.
		PublicDependencyModuleNames.AddRange(new string[]
		{
			"Core",
			"CoreUObject",
			"Engine",
			"Json",
		});

		// ── ThirdParty/Daimon layout ──────────────────────────────────────────
		string ThirdParty = Path.Combine(ModuleDirectory, "..", "ThirdParty", "Daimon");
		string IncludeDir = Path.Combine(ThirdParty, "include");
		string LibRoot    = Path.Combine(ThirdParty, "lib");

		// daimon.h lives here; DaimonAgent.cpp does #include "daimon.h".
		PublicIncludePaths.Add(IncludeDir);

		// ── DYNAMIC linkage, per platform ─────────────────────────────────────
		if (Target.Platform == UnrealTargetPlatform.Win64)
		{
			string LibDir = Path.Combine(LibRoot, "Win64");
			string Dll    = Path.Combine(LibDir, "daimon_ffi.dll");
			string ImpLib = Path.Combine(LibDir, "daimon_ffi.dll.lib"); // import lib

			// Link the import lib (resolves symbols at build time)…
			PublicAdditionalLibraries.Add(ImpLib);
			// …but DELAY-LOAD the DLL so we control when/where it loads, and copy
			// it next to the executable so the loader finds it.
			PublicDelayLoadDLLs.Add("daimon_ffi.dll");
			RuntimeDependencies.Add("$(BinaryOutputDir)/daimon_ffi.dll", Dll);

			// DAIMON_DLL_NAME lets DaimonAgent.cpp FPlatformProcess::GetDllHandle
			// the delay-loaded DLL explicitly (see DaimonAgent.cpp).
			PublicDefinitions.Add("DAIMON_DLL_NAME=TEXT(\"daimon_ffi.dll\")");
		}
		else if (Target.Platform == UnrealTargetPlatform.Mac)
		{
			string Dylib = Path.Combine(LibRoot, "Mac", "libdaimon_ffi.dylib");
			PublicAdditionalLibraries.Add(Dylib);
			RuntimeDependencies.Add("$(BinaryOutputDir)/libdaimon_ffi.dylib", Dylib);
			PublicDefinitions.Add("DAIMON_DLL_NAME=TEXT(\"libdaimon_ffi.dylib\")");
		}
		else if (Target.Platform == UnrealTargetPlatform.Linux)
		{
			string So = Path.Combine(LibRoot, "Linux", "libdaimon_ffi.so");
			PublicAdditionalLibraries.Add(So);
			RuntimeDependencies.Add("$(BinaryOutputDir)/libdaimon_ffi.so", So);
			PublicDefinitions.Add("DAIMON_DLL_NAME=TEXT(\"libdaimon_ffi.so\")");
		}

		// ── ALTERNATIVE: fully STATIC link ────────────────────────────────────
		// To link the static archive instead of the shared lib, drop
		// libdaimon_ffi.a (Mac/Linux) / daimon_ffi.lib (Win64) into the same lib
		// dirs, REMOVE the dynamic block above, and use only:
		//
		//   PublicAdditionalLibraries.Add(Path.Combine(LibRoot, PlatDir, ArchiveName));
		//
		// with NO PublicDelayLoadDLLs / RuntimeDependencies / DAIMON_DLL_NAME, and
		// build DaimonAgent.cpp with DAIMON_STATIC defined so it skips the manual
		// GetDllHandle (symbols are already in the executable). On macOS a static
		// Rust staticlib also needs the system libs it depends on; if the linker
		// complains, add: PublicSystemLibraries / PublicFrameworks as needed.
		// Static is simplest for shipping a single binary (and required on iOS),
		// but forces an editor relink whenever the engine .a changes.
	}
}
