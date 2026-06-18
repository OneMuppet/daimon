// DaimonAgent.h — RAII C++ wrapper + Blueprint-friendly USTRUCTs for the Daimon
// cognitive engine.
//
// This file owns the entire bridge between Unreal types and the flat-JSON C ABI
// declared in ThirdParty/Daimon/include/daimon.h. Game code should use
// FDaimonAgent (or UDaimonAgentComponent); it never touches the raw C ABI.
//
// MEMORY: FDaimonAgent owns the opaque DaimonAgent* and frees it in its dtor.
// Every char* the library returns is copied into an FString and then released
// with daimon_string_free. daimon_version() is a static string and is NEVER
// freed.
//
// STRINGS: FString <-> UTF-8. Inbound we pass FString through TCHAR_TO_UTF8 (a
// FTCHARToUTF8 conversion under the hood) to get a const char*. Outbound we copy
// the returned UTF-8 char* via UTF8_TO_TCHAR into an FString before freeing it.
//
// JSON: we use Unreal's built-in Json module (FJsonObject / FJsonSerializer) —
// no external dependency. The wire format is FLAT (plain scalars, no tagged
// unions), matching crates/daimon-ffi exactly.

#pragma once

#include "CoreMinimal.h"
#include "DaimonAgent.generated.h"

// ─────────────────────────────────────────────────────────────────────────────
// Flat USTRUCTs — one field per JSON scalar. Field names/types mirror the serde
// DTOs in crates/daimon-ffi/src/lib.rs (i32->int32, f32->float, u32->int64 where
// Blueprint can't hold uint32). Directions are FString ("north"/"south"/"east"/
// "west" or empty for null).
// ─────────────────────────────────────────────────────────────────────────────

/** The NPC's own body/state this tick (perception.body). */
USTRUCT(BlueprintType)
struct DAIMONCORE_API FDaimonBody
{
	GENERATED_BODY()

	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Daimon")
	int32 X = 0;

	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Daimon")
	int32 Y = 0;

	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Daimon")
	float Health = 1.0f;

	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Daimon")
	float Energy = 1.0f;

	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Daimon")
	float Hydration = 1.0f;

	/** 0 = open, 1 = fully enclosed (walled in). */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Daimon")
	float Enclosure = 0.0f;

	/** Season index (0..). */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Daimon")
	int32 Season = 0;

	/** Ticks until winter; leave very large (e.g. 1e30) for "never". */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Daimon")
	float WinterIn = 3.4e38f; // f32::MAX ~ "winter never"

	/** How much the NPC is carrying (0..). */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Daimon")
	float Carrying = 0.0f;

	/** Direction of the gap in the NPC's shelter, or empty for null. */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Daimon")
	FString ShelterGap;

	/** Suggested direction to gather, or empty for null. */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Daimon")
	FString GatherDir;

	/** Suggested direction to store, or empty for null. */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Daimon")
	FString StoreDir;
};

/** One thing the NPC can see this tick (perception.visible[i]). */
USTRUCT(BlueprintType)
struct DAIMONCORE_API FDaimonEntity
{
	GENERATED_BODY()

	/** Stable entity id. Stored as int64 because Blueprint has no uint32. */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Daimon")
	int64 Id = 0;

	/** One of: food | water | agent | predator | curio. */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Daimon")
	FString Kind;

	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Daimon")
	int32 X = 0;

	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Daimon")
	int32 Y = 0;

	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Daimon")
	FString Label;
};

/** One world event delivered to the NPC this tick (perception.events[i]).
 *  kind is one of: ate | drank | hurt | repelled | heard | discovered |
 *  vanished | died | told (the complete inbound set). Only the fields relevant to
 *  a kind need be set; the engine ignores the rest (and ignores unknown kinds
 *  entirely). "told" is inter-agent info sharing the listener can act on (unlike
 *  sentiment-only "heard"): set Info = greeting|resource_at|danger_at, and for
 *  resource_at also Id/EntityKind/X/Y/Label; for danger_at also X/Y. */
USTRUCT(BlueprintType)
struct DAIMONCORE_API FDaimonEvent
{
	GENERATED_BODY()

	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Daimon")
	FString Kind;

	/** Subject entity id (ate/drank/hurt/repelled/discovered/vanished/died). */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Daimon")
	int64 Id = 0;

	/** Speaker id for "heard". */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Daimon")
	int64 From = 0;

	/** Energy gained for "ate". */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Daimon")
	float Energy = 0.0f;

	/** Health for "hurt". */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Daimon")
	float Health = 0.0f;

	/** Location for "died". */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Daimon")
	int32 X = 0;

	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Daimon")
	int32 Y = 0;

	/** Spoken text for "heard". */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Daimon")
	FString Text;

	/** Cause for "died". */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Daimon")
	FString Cause;

	/** "told": greeting | resource_at | danger_at. */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Daimon")
	FString Info;

	/** "told"+resource_at: the shared entity's kind (food|water|agent|predator|curio). */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Daimon")
	FString EntityKind;

	/** "told"+resource_at: the shared resource's label. */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Daimon")
	FString Label;
};

/** A full tick of perception fed to FDaimonAgent::Think. */
USTRUCT(BlueprintType)
struct DAIMONCORE_API FDaimonPerception
{
	GENERATED_BODY()

	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Daimon")
	FDaimonBody Body;

	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Daimon")
	TArray<FDaimonEntity> Visible;

	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Daimon")
	TArray<FDaimonEvent> Events;
};

/** The decision returned by FDaimonAgent::Think (the flat decision object).
 *
 *  Read by Action:
 *    move                         -> Dir
 *    eat|drink|inspect|strike|talk-> Target (entity id)
 *    talk                         -> Target and Text
 *    build                        -> PosX, PosY (bHasPos == true)
 *    gather|store|rest|wait       -> (no extra fields)
 */
USTRUCT(BlueprintType)
struct DAIMONCORE_API FDaimonThought
{
	GENERATED_BODY()

	/** move|eat|drink|talk|inspect|strike|build|gather|store|rest|wait. */
	UPROPERTY(BlueprintReadOnly, Category = "Daimon")
	FString Action;

	/** For move: north|south|east|west. Empty otherwise. */
	UPROPERTY(BlueprintReadOnly, Category = "Daimon")
	FString Dir;

	/** Entity id for eat/drink/inspect/strike/talk; valid only if bHasTarget. */
	UPROPERTY(BlueprintReadOnly, Category = "Daimon")
	int64 Target = 0;

	UPROPERTY(BlueprintReadOnly, Category = "Daimon")
	bool bHasTarget = false;

	/** Build location; valid only if bHasPos. */
	UPROPERTY(BlueprintReadOnly, Category = "Daimon")
	int32 PosX = 0;

	UPROPERTY(BlueprintReadOnly, Category = "Daimon")
	int32 PosY = 0;

	UPROPERTY(BlueprintReadOnly, Category = "Daimon")
	bool bHasPos = false;

	/** Utterance for talk. */
	UPROPERTY(BlueprintReadOnly, Category = "Daimon")
	FString Text;

	/** Current goal label (e.g. "forage"). */
	UPROPERTY(BlueprintReadOnly, Category = "Daimon")
	FString Goal;

	/** Dominant drive (e.g. "hunger"). */
	UPROPERTY(BlueprintReadOnly, Category = "Daimon")
	FString Drive;

	/** Cognitive process: reflex | routine | deliberate. */
	UPROPERTY(BlueprintReadOnly, Category = "Daimon")
	FString Process;

	/** First-person inner monologue (flavour / debugging). */
	UPROPERTY(BlueprintReadOnly, Category = "Daimon")
	FString Inner;
};

// ─────────────────────────────────────────────────────────────────────────────
// FDaimonAgent — RAII wrapper over one opaque DaimonAgent*.
// ─────────────────────────────────────────────────────────────────────────────

/**
 * Owns one Daimon mind. Construct with New (or Load); destruction frees the
 * native handle. Non-copyable, movable. Not thread-safe per instance — tick a
 * given agent from one thread; tick distinct agents in a fixed order for
 * determinism (see README determinism rules).
 */
class DAIMONCORE_API FDaimonAgent
{
public:
	/** Spawn from a persona JSON string (all fields optional, e.g.
	 *  {"name":"Mara","curiosity":0.9}) and a DISTINCT per-NPC seed.
	 *  Returns nullptr on error (call LastError for the message). */
	static TUniquePtr<FDaimonAgent> New(const FString& PersonaJson, uint64 Seed);

	/** Restore a mind previously produced by Save(). Returns nullptr on error. */
	static TUniquePtr<FDaimonAgent> Load(const FString& Name, const FString& Json);

	/** Advance the mind one tick. On a native error returns a thought whose
	 *  Action == "wait" with Inner describing the error. */
	FDaimonThought Think(const FDaimonPerception& Perception);

	/** Serialise the mind to a JSON save string (empty on error). */
	FString Save() const;

	/** The native library version (static string, never freed). */
	static FString Version();

	/** Last error on this thread, or empty. Reading it clears it natively. */
	static FString LastError();

	~FDaimonAgent();

	// Non-copyable, movable.
	FDaimonAgent(const FDaimonAgent&) = delete;
	FDaimonAgent& operator=(const FDaimonAgent&) = delete;
	FDaimonAgent(FDaimonAgent&& Other) noexcept;
	FDaimonAgent& operator=(FDaimonAgent&& Other) noexcept;

	bool IsValid() const { return Handle != nullptr; }

private:
	explicit FDaimonAgent(struct DaimonAgent* InHandle) : Handle(InHandle) {}

	struct DaimonAgent* Handle = nullptr;
};
