// DaimonAgent.cpp — bridge between Unreal types and the Daimon flat-JSON C ABI.
//
// Mirrors crates/daimon-ffi/include/daimon.h exactly. All memory rules live
// here: every char* the library returns is copied into an FString and released
// with daimon_string_free; daimon_version() is static and never freed; the
// opaque handle is freed in the dtor.

#include "DaimonAgent.h"

#include "Dom/JsonObject.h"
#include "Dom/JsonValue.h"
#include "Serialization/JsonReader.h"
#include "Serialization/JsonSerializer.h"
#include "Serialization/JsonWriter.h"

// ─────────────────────────────────────────────────────────────────────────────
// The C ABI (faithful to daimon.h). We declare it here rather than including the
// header so that a STATIC build still resolves these symbols from the archive
// and a DYNAMIC build resolves them through the delay-loaded import. (Including
// daimon.h would also work — PublicIncludePaths adds it — but a local extern "C"
// keeps this translation unit self-describing.)
// ─────────────────────────────────────────────────────────────────────────────
extern "C"
{
	typedef struct DaimonAgent DaimonAgent;

	DaimonAgent* daimon_agent_new(const char* persona_json, uint64 seed);
	char*        daimon_agent_think(DaimonAgent* agent, const char* input_json);
	char*        daimon_agent_save(DaimonAgent* agent);
	DaimonAgent* daimon_agent_load(const char* name, const char* mind_json);
	void         daimon_agent_free(DaimonAgent* agent);
	void         daimon_string_free(char* s);
	const char*  daimon_version(void);
	char*        daimon_last_error(void);
}

// ─────────────────────────────────────────────────────────────────────────────
// FString <-> UTF-8 helpers.
// ─────────────────────────────────────────────────────────────────────────────
namespace
{
	/** Copy a UTF-8 char* returned by the library into an FString, then free it
	 *  with daimon_string_free. Returns empty for null. NEVER use this on
	 *  daimon_version() (that string is static — see FDaimonAgent::Version). */
	FString TakeOwnedCString(char* Owned)
	{
		if (Owned == nullptr)
		{
			return FString();
		}
		FString Result = UTF8_TO_TCHAR(Owned); // copies into FString storage
		daimon_string_free(Owned);             // release the library allocation
		return Result;
	}
}

// ─────────────────────────────────────────────────────────────────────────────
// Perception (FDaimonPerception -> flat JSON string).
// ─────────────────────────────────────────────────────────────────────────────
namespace
{
	/** Set a string field, or null if Empty (matches the engine's Option<…>). */
	void SetStrOrNull(const TSharedRef<FJsonObject>& Obj, const TCHAR* Key, const FString& Value)
	{
		if (Value.IsEmpty())
		{
			// MakeShareable(new …) is the conventional, always-valid way to wrap
			// an FJsonValue subclass in a TSharedPtr<FJsonValue>.
			Obj->SetField(Key, MakeShareable(new FJsonValueNull()));
		}
		else
		{
			Obj->SetStringField(Key, Value);
		}
	}

	FString BuildPerceptionJson(const FDaimonPerception& P)
	{
		const TSharedRef<FJsonObject> Root = MakeShared<FJsonObject>();

		// body
		{
			const TSharedRef<FJsonObject> Body = MakeShared<FJsonObject>();
			Body->SetNumberField(TEXT("x"), P.Body.X);
			Body->SetNumberField(TEXT("y"), P.Body.Y);
			Body->SetNumberField(TEXT("health"), P.Body.Health);
			Body->SetNumberField(TEXT("energy"), P.Body.Energy);
			Body->SetNumberField(TEXT("hydration"), P.Body.Hydration);
			Body->SetNumberField(TEXT("enclosure"), P.Body.Enclosure);
			Body->SetNumberField(TEXT("season"), P.Body.Season);
			Body->SetNumberField(TEXT("winter_in"), P.Body.WinterIn);
			Body->SetNumberField(TEXT("carrying"), P.Body.Carrying);
			SetStrOrNull(Body, TEXT("shelter_gap"), P.Body.ShelterGap);
			SetStrOrNull(Body, TEXT("gather_dir"), P.Body.GatherDir);
			SetStrOrNull(Body, TEXT("store_dir"), P.Body.StoreDir);
			Root->SetObjectField(TEXT("body"), Body);
		}

		// visible
		{
			TArray<TSharedPtr<FJsonValue>> Arr;
			Arr.Reserve(P.Visible.Num());
			for (const FDaimonEntity& E : P.Visible)
			{
				const TSharedRef<FJsonObject> O = MakeShared<FJsonObject>();
				O->SetNumberField(TEXT("id"), static_cast<double>(E.Id));
				O->SetStringField(TEXT("kind"), E.Kind);
				O->SetNumberField(TEXT("x"), E.X);
				O->SetNumberField(TEXT("y"), E.Y);
				O->SetStringField(TEXT("label"), E.Label);
				Arr.Add(MakeShareable(new FJsonValueObject(O)));
			}
			Root->SetArrayField(TEXT("visible"), Arr);
		}

		// events
		{
			TArray<TSharedPtr<FJsonValue>> Arr;
			Arr.Reserve(P.Events.Num());
			for (const FDaimonEvent& Ev : P.Events)
			{
				const TSharedRef<FJsonObject> O = MakeShared<FJsonObject>();
				O->SetStringField(TEXT("kind"), Ev.Kind);
				O->SetNumberField(TEXT("id"), static_cast<double>(Ev.Id));
				O->SetNumberField(TEXT("from"), static_cast<double>(Ev.From));
				O->SetNumberField(TEXT("energy"), Ev.Energy);
				O->SetNumberField(TEXT("health"), Ev.Health);
				O->SetNumberField(TEXT("x"), Ev.X);
				O->SetNumberField(TEXT("y"), Ev.Y);
				O->SetStringField(TEXT("text"), Ev.Text);
				O->SetStringField(TEXT("cause"), Ev.Cause);
				O->SetStringField(TEXT("info"), Ev.Info);
				O->SetStringField(TEXT("entity_kind"), Ev.EntityKind);
				O->SetStringField(TEXT("label"), Ev.Label);
				Arr.Add(MakeShareable(new FJsonValueObject(O)));
			}
			Root->SetArrayField(TEXT("events"), Arr);
		}

		FString Out;
		const TSharedRef<TJsonWriter<>> Writer = TJsonWriterFactory<>::Create(&Out);
		FJsonSerializer::Serialize(Root, Writer);
		return Out;
	}

	// ─── Decision JSON string -> FDaimonThought ──────────────────────────────
	FDaimonThought ParseThoughtJson(const FString& Json)
	{
		FDaimonThought T;

		TSharedPtr<FJsonObject> Root;
		const TSharedRef<TJsonReader<>> Reader = TJsonReaderFactory<>::Create(Json);
		if (!FJsonSerializer::Deserialize(Reader, Root) || !Root.IsValid())
		{
			T.Action = TEXT("wait");
			T.Inner = FString::Printf(TEXT("DaimonCore: could not parse decision JSON: %s"), *Json);
			return T;
		}

		Root->TryGetStringField(TEXT("action"), T.Action);
		Root->TryGetStringField(TEXT("goal"), T.Goal);
		Root->TryGetStringField(TEXT("drive"), T.Drive);
		Root->TryGetStringField(TEXT("process"), T.Process);
		Root->TryGetStringField(TEXT("inner"), T.Inner);

		// dir: string or null
		if (Root->HasTypedField<EJson::String>(TEXT("dir")))
		{
			Root->TryGetStringField(TEXT("dir"), T.Dir);
		}

		// target: number (entity id) or null
		if (Root->HasTypedField<EJson::Number>(TEXT("target")))
		{
			double TargetNum = 0.0;
			Root->TryGetNumberField(TEXT("target"), TargetNum);
			T.Target = static_cast<int64>(TargetNum);
			T.bHasTarget = true;
		}

		// text: string or null
		if (Root->HasTypedField<EJson::String>(TEXT("text")))
		{
			Root->TryGetStringField(TEXT("text"), T.Text);
		}

		// pos: [x, y] array or null
		const TArray<TSharedPtr<FJsonValue>>* PosArr = nullptr;
		if (Root->TryGetArrayField(TEXT("pos"), PosArr) && PosArr != nullptr && PosArr->Num() >= 2)
		{
			T.PosX = static_cast<int32>((*PosArr)[0]->AsNumber());
			T.PosY = static_cast<int32>((*PosArr)[1]->AsNumber());
			T.bHasPos = true;
		}

		return T;
	}
}

// ─────────────────────────────────────────────────────────────────────────────
// FDaimonAgent
// ─────────────────────────────────────────────────────────────────────────────

TUniquePtr<FDaimonAgent> FDaimonAgent::New(const FString& PersonaJson, uint64 Seed)
{
	// Inbound FString -> UTF-8 const char* (borrowed/copied by the library).
	const auto PersonaUtf8 = StringCast<UTF8CHAR>(*PersonaJson);
	DaimonAgent* Raw = daimon_agent_new(
		reinterpret_cast<const char*>(PersonaUtf8.Get()), Seed);
	if (Raw == nullptr)
	{
		UE_LOG(LogTemp, Error, TEXT("daimon_agent_new failed: %s"), *FDaimonAgent::LastError());
		return nullptr;
	}
	return TUniquePtr<FDaimonAgent>(new FDaimonAgent(Raw));
}

TUniquePtr<FDaimonAgent> FDaimonAgent::Load(const FString& Name, const FString& Json)
{
	const auto NameUtf8 = StringCast<UTF8CHAR>(*Name);
	const auto JsonUtf8 = StringCast<UTF8CHAR>(*Json);
	DaimonAgent* Raw = daimon_agent_load(
		reinterpret_cast<const char*>(NameUtf8.Get()),
		reinterpret_cast<const char*>(JsonUtf8.Get()));
	if (Raw == nullptr)
	{
		UE_LOG(LogTemp, Error, TEXT("daimon_agent_load failed: %s"), *FDaimonAgent::LastError());
		return nullptr;
	}
	return TUniquePtr<FDaimonAgent>(new FDaimonAgent(Raw));
}

FDaimonThought FDaimonAgent::Think(const FDaimonPerception& Perception)
{
	if (Handle == nullptr)
	{
		FDaimonThought T;
		T.Action = TEXT("wait");
		T.Inner = TEXT("DaimonCore: Think called on an invalid agent");
		return T;
	}

	const FString InputJson = BuildPerceptionJson(Perception);
	const auto InputUtf8 = StringCast<UTF8CHAR>(*InputJson);

	char* DecisionRaw = daimon_agent_think(
		Handle, reinterpret_cast<const char*>(InputUtf8.Get()));

	// Copy out + free in one step; empty if the call returned null.
	const FString DecisionJson = TakeOwnedCString(DecisionRaw);
	if (DecisionJson.IsEmpty())
	{
		FDaimonThought T;
		T.Action = TEXT("wait");
		T.Inner = FString::Printf(TEXT("daimon_agent_think failed: %s"), *FDaimonAgent::LastError());
		return T;
	}
	return ParseThoughtJson(DecisionJson);
}

FString FDaimonAgent::Save() const
{
	if (Handle == nullptr)
	{
		return FString();
	}
	return TakeOwnedCString(daimon_agent_save(Handle));
}

FString FDaimonAgent::Version()
{
	// daimon_version() returns a STATIC string — copy it but DO NOT free it.
	const char* Static = daimon_version();
	return (Static != nullptr) ? FString(UTF8_TO_TCHAR(Static)) : FString();
}

FString FDaimonAgent::LastError()
{
	// daimon_last_error() returns an owned char* (or null) and clears the slot.
	return TakeOwnedCString(daimon_last_error());
}

FDaimonAgent::~FDaimonAgent()
{
	if (Handle != nullptr)
	{
		daimon_agent_free(Handle); // NULL-safe natively, but we guard anyway
		Handle = nullptr;
	}
}

FDaimonAgent::FDaimonAgent(FDaimonAgent&& Other) noexcept
	: Handle(Other.Handle)
{
	Other.Handle = nullptr;
}

FDaimonAgent& FDaimonAgent::operator=(FDaimonAgent&& Other) noexcept
{
	if (this != &Other)
	{
		if (Handle != nullptr)
		{
			daimon_agent_free(Handle);
		}
		Handle = Other.Handle;
		Other.Handle = nullptr;
	}
	return *this;
}
