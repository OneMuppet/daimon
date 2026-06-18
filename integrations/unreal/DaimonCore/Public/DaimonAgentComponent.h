// DaimonAgentComponent.h — drop-on-an-Actor component that owns one Daimon mind
// and exposes Think to Blueprints, so designers can wire NPCs without C++.

#pragma once

#include "CoreMinimal.h"
#include "Components/ActorComponent.h"
#include "DaimonAgent.h"
#include "DaimonAgentComponent.generated.h"

UCLASS(ClassGroup = (Daimon), meta = (BlueprintSpawnableComponent),
	   DisplayName = "Daimon Agent")
class DAIMONCORE_API UDaimonAgentComponent : public UActorComponent
{
	GENERATED_BODY()

public:
	UDaimonAgentComponent();

	/** Persona JSON (all fields optional), e.g.
	 *  {"name":"Mara","boldness":0.6,"curiosity":0.9,"creed":"..."}. */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Daimon")
	FString PersonaJson = TEXT("{\"name\":\"Agent\"}");

	/** DISTINCT per-NPC seed — two agents sharing a seed behave identically the
	 *  moment they share a percept. Set a different value per placed NPC. */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Daimon")
	int64 Seed = 0;

	/** If true, spawn the mind in BeginPlay from PersonaJson + Seed. */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Daimon")
	bool bSpawnOnBeginPlay = true;

	/** (Re)create the mind from PersonaJson + Seed. Returns false on error. */
	UFUNCTION(BlueprintCallable, Category = "Daimon")
	bool SpawnAgent();

	/** Restore a mind from a Save() string instead of spawning fresh. */
	UFUNCTION(BlueprintCallable, Category = "Daimon")
	bool LoadAgent(const FString& Name, const FString& MindJson);

	/** Advance one tick. If no mind exists, returns a "wait" thought. */
	UFUNCTION(BlueprintCallable, Category = "Daimon")
	FDaimonThought Think(const FDaimonPerception& Perception);

	/** Serialise the mind for save games (empty if none). */
	UFUNCTION(BlueprintCallable, Category = "Daimon")
	FString Save() const;

	/** True once a mind has been created. */
	UFUNCTION(BlueprintPure, Category = "Daimon")
	bool HasAgent() const { return Agent.IsValid(); }

	/** Native library version string. */
	UFUNCTION(BlueprintPure, Category = "Daimon")
	static FString DaimonVersion() { return FDaimonAgent::Version(); }

protected:
	virtual void BeginPlay() override;
	virtual void EndPlay(const EEndPlayReason::Type EndPlayReason) override;

private:
	// RAII handle; freed automatically (TUniquePtr) on EndPlay / destruction.
	TUniquePtr<FDaimonAgent> Agent;
};
