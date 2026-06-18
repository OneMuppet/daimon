// DaimonNpcActor.h — example NPC driven by a Daimon mind.
//
// This is a documented example, NOT part of the DaimonCore module. Copy it into
// your own game module (one that lists "DaimonCore" in its dependencies) and
// fill in the // TODO: your game stubs.

#pragma once

#include "CoreMinimal.h"
#include "GameFramework/Actor.h"
#include "DaimonAgent.h"
#include "DaimonNpcActor.generated.h"

class UDaimonAgentComponent;

UCLASS()
class ADaimonNpcActor : public AActor
{
	GENERATED_BODY()

public:
	ADaimonNpcActor();

	/** The mind. Set a DISTINCT Seed per placed NPC (see BeginPlay). */
	UPROPERTY(VisibleAnywhere, Category = "Daimon")
	UDaimonAgentComponent* Mind;

protected:
	virtual void BeginPlay() override;
	virtual void Tick(float DeltaSeconds) override;

private:
	FDaimonPerception SensePerception() const;
	void ApplyThought(const FDaimonThought& Thought);

	// Grid position in your world (the engine works in integer grid cells).
	int32 GridX = 0;
	int32 GridY = 0;
};
