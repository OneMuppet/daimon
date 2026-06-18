// DaimonCoreModule.h — the UE module interface for DaimonCore.
//
// On a DYNAMIC link we load the native daimon-ffi shared library on module
// startup and free it on shutdown, so the delay-loaded import is resolved before
// any FDaimonAgent call. On a STATIC link (DAIMON_STATIC) this is a no-op.

#pragma once

#include "CoreMinimal.h"
#include "Modules/ModuleManager.h"

class FDaimonCoreModule : public IModuleInterface
{
public:
	virtual void StartupModule() override;
	virtual void ShutdownModule() override;

private:
	// Handle to the loaded daimon-ffi shared library (dynamic link only).
	void* DaimonLibHandle = nullptr;
};
