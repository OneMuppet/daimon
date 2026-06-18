// DaimonNpcActor.cpp — example: build a perception each tick, call Think, apply
// the action. Copy into your own game module and replace the // TODO stubs.

#include "DaimonNpcActor.h"
#include "DaimonAgentComponent.h"

ADaimonNpcActor::ADaimonNpcActor()
{
	PrimaryActorTick.bCanEverTick = true;
	Mind = CreateDefaultSubobject<UDaimonAgentComponent>(TEXT("DaimonMind"));
	Mind->PersonaJson = TEXT("{\"name\":\"Mara\",\"boldness\":0.6,\"curiosity\":0.9}");
}

void ADaimonNpcActor::BeginPlay()
{
	Super::BeginPlay();

	// DETERMINISM: give each NPC a DISTINCT, STABLE seed. Hashing a stable
	// identity (the actor's GUID/name) makes the seed reproducible across runs.
	// Do NOT use FMath::Rand() — that breaks determinism between sessions.
	const uint64 PerNpcSeed = GetTypeHash(GetFName());
	Mind->Seed = static_cast<int64>(PerNpcSeed);
	Mind->SpawnAgent(); // (bSpawnOnBeginPlay also would, after this seed is set)

	// TODO: your game — initialise GridX/GridY from this actor's world location.
}

void ADaimonNpcActor::Tick(float DeltaSeconds)
{
	Super::Tick(DeltaSeconds);

	// 1) WORLD -> PERCEPTION
	const FDaimonPerception Perception = SensePerception();

	// 2) THINK
	const FDaimonThought Thought = Mind->Think(Perception);

	// 3) ACTION -> WORLD
	ApplyThought(Thought);
}

// ── Mapping 1: your world -> perception ─────────────────────────────────────
FDaimonPerception ADaimonNpcActor::SensePerception() const
{
	FDaimonPerception P;

	// body — required fields x,y; the rest default sensibly if left as-is.
	P.Body.X = GridX;
	P.Body.Y = GridY;
	// TODO: your game — fill Health/Energy/Hydration from your stats (0..1).
	P.Body.Health    = 1.0f;
	P.Body.Energy    = 1.0f;
	P.Body.Hydration = 1.0f;
	// TODO: your game — season/winter_in/enclosure/carrying if you model them.

	// visible — one FDaimonEntity per thing this NPC currently perceives.
	// kind must be one of: food | water | agent | predator | curio.
	// TODO: your game — query nearby actors and append entities, e.g.:
	//   FDaimonEntity Berry;
	//   Berry.Id = OtherActorStableId; Berry.Kind = TEXT("food");
	//   Berry.X = OtherGridX; Berry.Y = OtherGridY; Berry.Label = TEXT("berry");
	//   P.Visible.Add(Berry);

	// events — things that happened TO this NPC since last tick.
	// kind: ate|drank|hurt|repelled|heard|discovered|vanished|died|told.
	// TODO: your game — drain your per-NPC event queue into P.Events, e.g.:
	//   FDaimonEvent Hurt; Hurt.Kind = TEXT("hurt");
	//   Hurt.Id = AttackerId; Hurt.Health = 0.2f; P.Events.Add(Hurt);

	return P;
}

// ── Mapping 2: returned action -> effects in your world ─────────────────────
void ADaimonNpcActor::ApplyThought(const FDaimonThought& T)
{
	// T.Inner is the inner monologue (great for a debug overlay).
	UE_LOG(LogTemp, Verbose, TEXT("[%s] %s (%s/%s): %s"),
		*GetName(), *T.Action, *T.Drive, *T.Process, *T.Inner);

	if (T.Action == TEXT("move"))
	{
		// dir is north|south|east|west.
		int32 DX = 0, DY = 0;
		if      (T.Dir == TEXT("north")) DY = +1;
		else if (T.Dir == TEXT("south")) DY = -1;
		else if (T.Dir == TEXT("east"))  DX = +1;
		else if (T.Dir == TEXT("west"))  DX = -1;
		GridX += DX;
		GridY += DY;
		// TODO: your game — move the actor to the new grid cell (clamp/collide).
	}
	else if (T.Action == TEXT("eat") || T.Action == TEXT("drink") ||
	         T.Action == TEXT("inspect") || T.Action == TEXT("strike"))
	{
		if (T.bHasTarget)
		{
			// TODO: your game — look up the entity with id T.Target and apply the
			// interaction (consume food, drink water, inspect curio, attack).
		}
	}
	else if (T.Action == TEXT("talk"))
	{
		if (T.bHasTarget)
		{
			// TODO: your game — make this NPC say T.Text to entity T.Target
			// (deliver it back as a "heard" event on the listener next tick).
		}
	}
	else if (T.Action == TEXT("build"))
	{
		if (T.bHasPos)
		{
			// TODO: your game — place a wall/structure at (T.PosX, T.PosY).
		}
	}
	else if (T.Action == TEXT("gather") || T.Action == TEXT("store"))
	{
		// TODO: your game — pick up / deposit resources at this cell.
	}
	else if (T.Action == TEXT("rest") || T.Action == TEXT("wait"))
	{
		// No movement; energy may recover in your own stat model.
	}
}
