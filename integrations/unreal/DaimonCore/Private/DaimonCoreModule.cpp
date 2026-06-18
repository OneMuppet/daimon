// DaimonCoreModule.cpp — load/unload the native daimon-ffi shared library.

#include "DaimonCoreModule.h"
#include "Interfaces/IPluginManager.h" // (only if shipped as a plugin; harmless)
#include "Misc/Paths.h"
#include "HAL/PlatformProcess.h"

#define LOCTEXT_NAMESPACE "FDaimonCoreModule"

void FDaimonCoreModule::StartupModule()
{
#if !defined(DAIMON_STATIC) && defined(DAIMON_DLL_NAME)
	// The .Build.cs delay-loads the DLL on Windows; explicitly resolving the
	// handle here guarantees the loader can find it (it lives in BinaryOutputDir,
	// i.e. next to the executable, via RuntimeDependencies). On Mac/Linux this
	// also pins the dylib/so for the lifetime of the module.
	const FString LibName = DAIMON_DLL_NAME;
	DaimonLibHandle = FPlatformProcess::GetDllHandle(*LibName);
	if (DaimonLibHandle == nullptr)
	{
		UE_LOG(LogTemp, Error,
			TEXT("DaimonCore: failed to load native library '%s'. Did you copy it ")
			TEXT("to Binaries (RuntimeDependencies) and build daimon-ffi --release?"),
			*LibName);
	}
#endif
}

void FDaimonCoreModule::ShutdownModule()
{
#if !defined(DAIMON_STATIC)
	if (DaimonLibHandle != nullptr)
	{
		FPlatformProcess::FreeDllHandle(DaimonLibHandle);
		DaimonLibHandle = nullptr;
	}
#endif
}

#undef LOCTEXT_NAMESPACE

IMPLEMENT_MODULE(FDaimonCoreModule, DaimonCore)
