// DaimonAgentComponent.cpp

#include "DaimonAgentComponent.h"

UDaimonAgentComponent::UDaimonAgentComponent()
{
	// The component does not tick itself — the owning Actor decides when to build
	// a perception and call Think (usually in its own Tick). Keep this off to
	// avoid surprising designers and to leave tick ordering to the game.
	PrimaryComponentTick.bCanEverTick = false;
}

void UDaimonAgentComponent::BeginPlay()
{
	Super::BeginPlay();
	if (bSpawnOnBeginPlay && !Agent.IsValid())
	{
		SpawnAgent();
	}
}

void UDaimonAgentComponent::EndPlay(const EEndPlayReason::Type EndPlayReason)
{
	Agent.Reset(); // frees the native handle via FDaimonAgent::~FDaimonAgent
	Super::EndPlay(EndPlayReason);
}

bool UDaimonAgentComponent::SpawnAgent()
{
	Agent = FDaimonAgent::New(PersonaJson, static_cast<uint64>(Seed));
	return Agent.IsValid();
}

bool UDaimonAgentComponent::LoadAgent(const FString& Name, const FString& MindJson)
{
	Agent = FDaimonAgent::Load(Name, MindJson);
	return Agent.IsValid();
}

FDaimonThought UDaimonAgentComponent::Think(const FDaimonPerception& Perception)
{
	if (!Agent.IsValid())
	{
		FDaimonThought T;
		T.Action = TEXT("wait");
		T.Inner = TEXT("DaimonCore: no agent (call SpawnAgent or LoadAgent first)");
		return T;
	}
	return Agent->Think(Perception);
}

FString UDaimonAgentComponent::Save() const
{
	return Agent.IsValid() ? Agent->Save() : FString();
}
