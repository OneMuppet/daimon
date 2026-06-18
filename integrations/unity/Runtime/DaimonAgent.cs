// DaimonAgent.cs — idiomatic, zero-dependency C# wrapper around the Daimon C ABI.
//
// Responsibilities:
//   * Own the native handle; free it deterministically (Dispose) and as a safety
//     net (finalizer).
//   * Enforce the one memory rule: every char* the library RETURNS (think / save /
//     last_error) is copied to a managed string then freed with daimon_string_free.
//     daimon_version() is the lone exception — it is a static string, never freed.
//   * Marshal UTF-8 in both directions (see DaimonNative.cs for the param attrs).
//
// JSON is FLAT (plain scalar fields, no tagged unions), so the tiny built-in
// builder/parser below is enough. No Newtonsoft / System.Text.Json dependency —
// older Unity (pre-2021 .NET) lacks System.Text.Json. See README for swapping in
// Newtonsoft if you prefer.

using System;
using System.Collections.Generic;
using System.Globalization;
using System.Runtime.InteropServices;
using System.Text;
using System.Threading;

namespace Daimon
{
    /// <summary>
    /// One Daimon mind. Spawn one per NPC with a DISTINCT seed; call
    /// <see cref="Think"/> each tick. Implements <see cref="IDisposable"/>; wrap in
    /// `using` or call <see cref="Dispose"/> when the NPC is destroyed.
    /// </summary>
    public sealed class DaimonAgent : IDisposable
    {
        private IntPtr _handle;

        private DaimonAgent(IntPtr handle)
        {
            _handle = handle;
        }

        /// <summary>
        /// Spawn an agent from persona traits + a deterministic seed. All persona
        /// fields are optional on the native side; pass <c>null</c> creed to omit it.
        /// Throws <see cref="DaimonException"/> on failure.
        /// </summary>
        public DaimonAgent(
            string name,
            float boldness,
            float sociability,
            float curiosity,
            string creed,
            ulong seed)
        {
            string personaJson = BuildPersonaJson(name, boldness, sociability, curiosity, creed);
            _handle = DaimonNative.daimon_agent_new(personaJson, seed);
            if (_handle == IntPtr.Zero)
                throw new DaimonException("daimon_agent_new failed: " + LastError());
        }

        /// <summary>The native library version (the daimon-ffi crate version).</summary>
        public static string Version()
        {
            // Static string on the native side — DO NOT free it.
            IntPtr p = DaimonNative.daimon_version();
            return p == IntPtr.Zero ? string.Empty : Utf8.FromPtr(p);
        }

        /// <summary>
        /// Advance the mind one tick. Builds the flat perception JSON, calls the
        /// engine, parses the flat decision JSON into a <see cref="Thought"/>.
        /// Throws <see cref="DaimonException"/> on failure (e.g. disposed handle).
        /// </summary>
        public Thought Think(Perception p)
        {
            if (_handle == IntPtr.Zero)
                throw new ObjectDisposedException(nameof(DaimonAgent));

            string inputJson = p.ToJson();
            IntPtr ret = DaimonNative.daimon_agent_think(_handle, inputJson);
            string decisionJson = TakeString(ret);
            if (decisionJson == null)
                throw new DaimonException("daimon_agent_think failed: " + LastError());

            return Thought.Parse(decisionJson);
        }

        /// <summary>Serialise the mind to a JSON save string. Throws on failure.</summary>
        public string Save()
        {
            if (_handle == IntPtr.Zero)
                throw new ObjectDisposedException(nameof(DaimonAgent));

            string json = TakeString(DaimonNative.daimon_agent_save(_handle));
            if (json == null)
                throw new DaimonException("daimon_agent_save failed: " + LastError());
            return json;
        }

        /// <summary>Restore a mind from a <see cref="Save"/> string. Throws on failure.</summary>
        public static DaimonAgent Load(string name, string json)
        {
            IntPtr handle = DaimonNative.daimon_agent_load(name, json);
            if (handle == IntPtr.Zero)
                throw new DaimonException("daimon_agent_load failed: " + LastError());
            return new DaimonAgent(handle);
        }

        /// <summary>
        /// Drains the thread-local last-error string from the engine (and frees it).
        /// Returns "(none)" if there was no error.
        /// </summary>
        public static string LastError()
        {
            string msg = TakeString(DaimonNative.daimon_last_error());
            return string.IsNullOrEmpty(msg) ? "(none)" : msg;
        }

        /// <summary>
        /// Copy a library-returned UTF-8 char* into a managed string, then free the
        /// native buffer with daimon_string_free. Returns null for a null pointer.
        /// NEVER pass daimon_version()'s pointer here — that one is static.
        /// </summary>
        private static string TakeString(IntPtr ptr)
        {
            if (ptr == IntPtr.Zero)
                return null;
            try
            {
                return Utf8.FromPtr(ptr);
            }
            finally
            {
                DaimonNative.daimon_string_free(ptr);
            }
        }

        public void Dispose()
        {
            FreeHandle();
            GC.SuppressFinalize(this);
        }

        ~DaimonAgent()
        {
            // Safety net only — prefer explicit Dispose so the native mind is
            // released when the NPC dies rather than at an arbitrary GC.
            FreeHandle();
        }

        private void FreeHandle()
        {
            IntPtr h = Interlocked.Exchange(ref _handle, IntPtr.Zero);
            if (h != IntPtr.Zero)
                DaimonNative.daimon_agent_free(h); // NULL-safe on the native side
        }

        // ── persona JSON builder ────────────────────────────────────────────────

        private static string BuildPersonaJson(
            string name, float boldness, float sociability, float curiosity, string creed)
        {
            var sb = new StringBuilder(96);
            sb.Append('{');
            Json.Field(sb, "name", name, first: true);
            Json.Field(sb, "boldness", boldness);
            Json.Field(sb, "sociability", sociability);
            Json.Field(sb, "curiosity", curiosity);
            if (creed != null)
                Json.Field(sb, "creed", creed);
            sb.Append('}');
            return sb.ToString();
        }
    }

    /// <summary>Raised when a Daimon native call returns an error.</summary>
    public sealed class DaimonException : Exception
    {
        public DaimonException(string message) : base(message) { }
    }

    // ──────────────────────────────────────────────────────────────────────────
    //  Perception (input) — flat DTOs that serialise to the engine's perception
    //  JSON via ToJson(). Field names + defaults mirror crates/daimon-ffi/src/lib.rs.
    // ──────────────────────────────────────────────────────────────────────────

    /// <summary>The NPC's own body/state this tick.</summary>
    public struct Body
    {
        public int X;
        public int Y;
        public float Health;     // engine default 1.0
        public float Energy;     // engine default 1.0
        public float Hydration;  // engine default 1.0
        public float Enclosure;  // engine default 0.0
        public byte Season;      // engine default 0
        public float WinterIn;   // engine default f32::MAX (≈ "never")
        public float Carrying;   // engine default 0.0
        public string ShelterGap; // "north"|"south"|"east"|"west"|null
        public string GatherDir;  // dir or null
        public string StoreDir;   // dir or null

        /// <summary>A body with the engine's own defaults (full health/energy/etc).</summary>
        public static Body Default(int x, int y)
        {
            return new Body
            {
                X = x,
                Y = y,
                Health = 1f,
                Energy = 1f,
                Hydration = 1f,
                Enclosure = 0f,
                Season = 0,
                WinterIn = float.MaxValue,
                Carrying = 0f,
                ShelterGap = null,
                GatherDir = null,
                StoreDir = null,
            };
        }
    }

    /// <summary>An entity the NPC can see. <see cref="Kind"/> ∈ food|water|agent|predator|curio.</summary>
    public struct VisibleEntity
    {
        public uint Id;
        public string Kind;
        public int X;
        public int Y;
        public string Label; // optional; "" if none

        public VisibleEntity(uint id, string kind, int x, int y, string label = "")
        {
            Id = id;
            Kind = kind;
            X = x;
            Y = y;
            Label = label ?? "";
        }
    }

    /// <summary>
    /// Something that happened to the NPC this tick. <see cref="Kind"/> ∈
    /// ate|drank|hurt|repelled|heard|discovered|vanished|died|told (the complete
    /// inbound set). Only the fields the kind needs are read by the engine; leave
    /// the rest at defaults. <c>told</c> is inter-agent info sharing — content the
    /// listener can act on, unlike sentiment-only <c>heard</c>.
    /// </summary>
    public struct WorldEventDto
    {
        public string Kind;
        public uint Id;      // subject entity ("told"+resource_at: the resource id)
        public uint From;    // "heard"/"told": speaker
        public float Energy; // "ate": energy gained
        public float Health; // "hurt": new health
        public int X;        // "died"/"told": position
        public int Y;
        public string Text;  // "heard": utterance
        public string Cause; // "died": cause label
        public string Info;  // "told": greeting|resource_at|danger_at
        public string EntityKind; // "told"+resource_at: food|water|agent|predator|curio
        public string Label; // "told"+resource_at: resource label

        public static WorldEventDto Ate(uint id, float energy) =>
            new WorldEventDto { Kind = "ate", Id = id, Energy = energy };
        public static WorldEventDto Drank(uint id) =>
            new WorldEventDto { Kind = "drank", Id = id };
        public static WorldEventDto Hurt(uint id, float health) =>
            new WorldEventDto { Kind = "hurt", Id = id, Health = health };
        public static WorldEventDto Repelled(uint id) =>
            new WorldEventDto { Kind = "repelled", Id = id };
        public static WorldEventDto Heard(uint from, string text) =>
            new WorldEventDto { Kind = "heard", From = from, Text = text };
        public static WorldEventDto Discovered(uint id) =>
            new WorldEventDto { Kind = "discovered", Id = id };
        public static WorldEventDto Vanished(uint id) =>
            new WorldEventDto { Kind = "vanished", Id = id };
        public static WorldEventDto Died(uint id, int x, int y, string cause) =>
            new WorldEventDto { Kind = "died", Id = id, X = x, Y = y, Cause = cause };

        // Inter-agent information sharing (told to this NPC by `from`):
        public static WorldEventDto ToldGreeting(uint from) =>
            new WorldEventDto { Kind = "told", From = from, Info = "greeting" };
        public static WorldEventDto ToldResource(uint from, uint id, string entityKind, int x, int y, string label = "") =>
            new WorldEventDto { Kind = "told", From = from, Info = "resource_at", Id = id, EntityKind = entityKind, X = x, Y = y, Label = label };
        public static WorldEventDto ToldDanger(uint from, int x, int y) =>
            new WorldEventDto { Kind = "told", From = from, Info = "danger_at", X = x, Y = y };
    }

    /// <summary>
    /// One tick of perception. Build it from your scene, then pass to
    /// <see cref="DaimonAgent.Think"/>. <see cref="ToJson"/> emits the flat
    /// perception JSON the engine expects.
    /// </summary>
    public sealed class Perception
    {
        public Body Body;
        public List<VisibleEntity> Visible = new List<VisibleEntity>();
        public List<WorldEventDto> Events = new List<WorldEventDto>();

        public Perception() { }

        public Perception(Body body)
        {
            Body = body;
        }

        public string ToJson()
        {
            var sb = new StringBuilder(256);
            sb.Append("{\"body\":{");
            Json.Field(sb, "x", Body.X, first: true);
            Json.Field(sb, "y", Body.Y);
            Json.Field(sb, "health", Body.Health);
            Json.Field(sb, "energy", Body.Energy);
            Json.Field(sb, "hydration", Body.Hydration);
            Json.Field(sb, "enclosure", Body.Enclosure);
            Json.Field(sb, "season", Body.Season);
            Json.Field(sb, "winter_in", Body.WinterIn);
            Json.Field(sb, "carrying", Body.Carrying);
            Json.FieldOrNull(sb, "shelter_gap", Body.ShelterGap);
            Json.FieldOrNull(sb, "gather_dir", Body.GatherDir);
            Json.FieldOrNull(sb, "store_dir", Body.StoreDir);
            sb.Append('}'); // close body

            sb.Append(",\"visible\":[");
            for (int i = 0; i < Visible.Count; i++)
            {
                if (i > 0) sb.Append(',');
                VisibleEntity e = Visible[i];
                sb.Append('{');
                Json.Field(sb, "id", e.Id, first: true);
                Json.Field(sb, "kind", e.Kind ?? "curio");
                Json.Field(sb, "x", e.X);
                Json.Field(sb, "y", e.Y);
                Json.Field(sb, "label", e.Label ?? "");
                sb.Append('}');
            }
            sb.Append(']');

            sb.Append(",\"events\":[");
            for (int i = 0; i < Events.Count; i++)
            {
                if (i > 0) sb.Append(',');
                WorldEventDto ev = Events[i];
                sb.Append('{');
                Json.Field(sb, "kind", ev.Kind ?? "heard", first: true);
                Json.Field(sb, "id", ev.Id);
                Json.Field(sb, "from", ev.From);
                Json.Field(sb, "energy", ev.Energy);
                Json.Field(sb, "health", ev.Health);
                Json.Field(sb, "x", ev.X);
                Json.Field(sb, "y", ev.Y);
                Json.Field(sb, "text", ev.Text ?? "");
                Json.Field(sb, "cause", ev.Cause ?? "");
                Json.Field(sb, "info", ev.Info ?? "");
                Json.Field(sb, "entity_kind", ev.EntityKind ?? "");
                Json.Field(sb, "label", ev.Label ?? "");
                sb.Append('}');
            }
            sb.Append(']');

            sb.Append('}');
            return sb.ToString();
        }
    }

    // ──────────────────────────────────────────────────────────────────────────
    //  Thought (output) — parsed from the flat decision JSON.
    // ──────────────────────────────────────────────────────────────────────────

    /// <summary>
    /// The engine's decision for one tick. <see cref="Action"/> ∈
    /// move|eat|drink|talk|inspect|strike|build|gather|store|rest|wait.
    /// For <c>move</c> read <see cref="Dir"/>; for eat|drink|inspect|strike|talk
    /// read <see cref="Target"/> (entity id); for build read <see cref="Pos"/>
    /// ([x,y]); for talk also read <see cref="Text"/>.
    /// </summary>
    public sealed class Thought
    {
        public string Action;   // verb, always present
        public string Dir;      // move: north|south|east|west, else null
        public uint? Target;    // eat/drink/inspect/strike/talk: entity id
        public int[] Pos;       // build: [x, y]
        public string Text;     // talk: utterance
        public string Goal;     // current goal label
        public string Drive;    // dominant drive name
        public string Process;  // reflex|routine|deliberate
        public string Inner;    // inner-monologue line

        /// <summary>Parse the flat decision JSON returned by the engine.</summary>
        public static Thought Parse(string json)
        {
            var map = Json.ParseFlat(json);
            var t = new Thought
            {
                Action = Json.GetString(map, "action"),
                Dir = Json.GetString(map, "dir"),
                Text = Json.GetString(map, "text"),
                Goal = Json.GetString(map, "goal"),
                Drive = Json.GetString(map, "drive"),
                Process = Json.GetString(map, "process"),
                Inner = Json.GetString(map, "inner"),
            };

            uint target;
            if (Json.TryGetUInt(map, "target", out target))
                t.Target = target;

            t.Pos = Json.GetIntArray(map, "pos"); // null if absent/null
            return t;
        }

        public override string ToString()
        {
            return $"Thought(action={Action}, dir={Dir}, target={Target}, " +
                   $"goal={Goal}, drive={Drive}, process={Process})";
        }
    }

    // ──────────────────────────────────────────────────────────────────────────
    //  Json — a tiny, zero-dependency helper. Build side emits strict JSON; parse
    //  side is a TOLERANT reader for the engine's FLAT decision object (top-level
    //  scalar values + the one small "pos" array; no nested objects). This is NOT
    //  a general JSON parser — it relies on the decision shape being flat.
    // ──────────────────────────────────────────────────────────────────────────

    internal static class Json
    {
        // ---- build side --------------------------------------------------------

        internal static void Field(StringBuilder sb, string key, string value, bool first = false)
        {
            Comma(sb, first);
            Key(sb, key);
            WriteString(sb, value);
        }

        internal static void FieldOrNull(StringBuilder sb, string key, string value, bool first = false)
        {
            Comma(sb, first);
            Key(sb, key);
            if (value == null)
                sb.Append("null");
            else
                WriteString(sb, value);
        }

        internal static void Field(StringBuilder sb, string key, int value, bool first = false)
        {
            Comma(sb, first);
            Key(sb, key);
            sb.Append(value.ToString(CultureInfo.InvariantCulture));
        }

        internal static void Field(StringBuilder sb, string key, uint value, bool first = false)
        {
            Comma(sb, first);
            Key(sb, key);
            sb.Append(value.ToString(CultureInfo.InvariantCulture));
        }

        internal static void Field(StringBuilder sb, string key, byte value, bool first = false)
        {
            Comma(sb, first);
            Key(sb, key);
            sb.Append(value.ToString(CultureInfo.InvariantCulture));
        }

        internal static void Field(StringBuilder sb, string key, float value, bool first = false)
        {
            Comma(sb, first);
            Key(sb, key);
            WriteFloat(sb, value);
        }

        private static void Comma(StringBuilder sb, bool first)
        {
            if (!first) sb.Append(',');
        }

        private static void Key(StringBuilder sb, string key)
        {
            sb.Append('"').Append(key).Append("\":");
        }

        private static void WriteFloat(StringBuilder sb, float value)
        {
            // serde_json rejects NaN/Infinity; the engine uses f32::MAX as the
            // "winter never" sentinel, so map non-finite to a large finite value.
            if (float.IsNaN(value))
            {
                sb.Append('0');
                return;
            }
            if (float.IsPositiveInfinity(value)) value = float.MaxValue;
            else if (float.IsNegativeInfinity(value)) value = float.MinValue;

            // "R" round-trips; invariant culture so the decimal point is '.'.
            sb.Append(value.ToString("R", CultureInfo.InvariantCulture));
        }

        private static void WriteString(StringBuilder sb, string value)
        {
            sb.Append('"');
            foreach (char c in value)
            {
                switch (c)
                {
                    case '"': sb.Append("\\\""); break;
                    case '\\': sb.Append("\\\\"); break;
                    case '\b': sb.Append("\\b"); break;
                    case '\f': sb.Append("\\f"); break;
                    case '\n': sb.Append("\\n"); break;
                    case '\r': sb.Append("\\r"); break;
                    case '\t': sb.Append("\\t"); break;
                    default:
                        if (c < 0x20)
                            sb.Append("\\u").Append(((int)c).ToString("x4", CultureInfo.InvariantCulture));
                        else
                            sb.Append(c);
                        break;
                }
            }
            sb.Append('"');
        }

        // ---- parse side (flat only) -------------------------------------------

        /// <summary>
        /// Parse a flat JSON object into key -> raw-token strings. String values
        /// are returned unquoted and unescaped. The "pos" array is captured as its
        /// raw "[..]" token. Nested objects are not expected in the decision shape.
        /// </summary>
        internal static Dictionary<string, string> ParseFlat(string json)
        {
            var map = new Dictionary<string, string>(StringComparer.Ordinal);
            if (string.IsNullOrEmpty(json))
                return map;

            int i = 0;
            int n = json.Length;
            SkipWs(json, ref i);
            if (i < n && json[i] == '{') i++; // enter object

            while (i < n)
            {
                SkipWs(json, ref i);
                if (i >= n || json[i] == '}') break;

                if (json[i] != '"')
                {
                    // be tolerant: skip stray separators
                    i++;
                    continue;
                }

                string key = ReadString(json, ref i);
                SkipWs(json, ref i);
                if (i < n && json[i] == ':') i++;
                SkipWs(json, ref i);

                if (i >= n) break;
                char c = json[i];
                string value;
                if (c == '"')
                {
                    value = ReadString(json, ref i);
                }
                else if (c == '[')
                {
                    value = ReadBracketed(json, ref i, '[', ']');
                }
                else if (c == '{')
                {
                    // not expected in a flat decision, but skip safely
                    value = ReadBracketed(json, ref i, '{', '}');
                }
                else
                {
                    value = ReadBareToken(json, ref i); // number, true, false, null
                }

                map[key] = value;

                SkipWs(json, ref i);
                if (i < n && json[i] == ',') i++;
            }

            return map;
        }

        internal static string GetString(Dictionary<string, string> map, string key)
        {
            string v;
            if (!map.TryGetValue(key, out v)) return null;
            if (v == null || v == "null") return null;
            return v;
        }

        internal static bool TryGetUInt(Dictionary<string, string> map, string key, out uint value)
        {
            value = 0;
            string v;
            if (!map.TryGetValue(key, out v)) return false;
            if (v == null || v == "null") return false;
            return uint.TryParse(v, NumberStyles.Integer, CultureInfo.InvariantCulture, out value);
        }

        internal static int[] GetIntArray(Dictionary<string, string> map, string key)
        {
            string v;
            if (!map.TryGetValue(key, out v)) return null;
            if (v == null || v == "null") return null;
            v = v.Trim();
            if (v.Length < 2 || v[0] != '[' || v[v.Length - 1] != ']') return null;

            string inner = v.Substring(1, v.Length - 2).Trim();
            if (inner.Length == 0) return new int[0];

            string[] parts = inner.Split(',');
            var result = new int[parts.Length];
            for (int k = 0; k < parts.Length; k++)
            {
                int parsed;
                if (!int.TryParse(parts[k].Trim(), NumberStyles.Integer, CultureInfo.InvariantCulture, out parsed))
                    return null;
                result[k] = parsed;
            }
            return result;
        }

        private static void SkipWs(string s, ref int i)
        {
            while (i < s.Length)
            {
                char c = s[i];
                if (c == ' ' || c == '\t' || c == '\n' || c == '\r') i++;
                else break;
            }
        }

        // Reads a JSON string starting at the opening quote; returns the decoded
        // contents and advances past the closing quote.
        private static string ReadString(string s, ref int i)
        {
            var sb = new StringBuilder();
            i++; // skip opening quote
            while (i < s.Length)
            {
                char c = s[i++];
                if (c == '"') break;
                if (c == '\\' && i < s.Length)
                {
                    char e = s[i++];
                    switch (e)
                    {
                        case '"': sb.Append('"'); break;
                        case '\\': sb.Append('\\'); break;
                        case '/': sb.Append('/'); break;
                        case 'b': sb.Append('\b'); break;
                        case 'f': sb.Append('\f'); break;
                        case 'n': sb.Append('\n'); break;
                        case 'r': sb.Append('\r'); break;
                        case 't': sb.Append('\t'); break;
                        case 'u':
                            if (i + 4 <= s.Length)
                            {
                                string hex = s.Substring(i, 4);
                                int code;
                                if (int.TryParse(hex, NumberStyles.HexNumber, CultureInfo.InvariantCulture, out code))
                                    sb.Append((char)code);
                                i += 4;
                            }
                            break;
                        default: sb.Append(e); break;
                    }
                }
                else
                {
                    sb.Append(c);
                }
            }
            return sb.ToString();
        }

        // Captures a bracketed token (array/object) verbatim, respecting nesting
        // and quoted strings. Returns the raw "[...]" / "{...}" substring.
        private static string ReadBracketed(string s, ref int i, char open, char close)
        {
            int start = i;
            int depth = 0;
            bool inStr = false;
            while (i < s.Length)
            {
                char c = s[i++];
                if (inStr)
                {
                    if (c == '\\') { if (i < s.Length) i++; }
                    else if (c == '"') inStr = false;
                    continue;
                }
                if (c == '"') { inStr = true; continue; }
                if (c == open) depth++;
                else if (c == close)
                {
                    depth--;
                    if (depth == 0) break;
                }
            }
            return s.Substring(start, i - start);
        }

        // Reads a bare token (number / true / false / null) up to the next
        // delimiter; returns it trimmed.
        private static string ReadBareToken(string s, ref int i)
        {
            int start = i;
            while (i < s.Length)
            {
                char c = s[i];
                if (c == ',' || c == '}' || c == ']' || c == ' ' || c == '\t' || c == '\n' || c == '\r')
                    break;
                i++;
            }
            return s.Substring(start, i - start).Trim();
        }
    }

    // Note: System.Threading.Interlocked (used in DaimonAgent.FreeHandle for the
    // dispose race) is part of the base class library in every Unity .NET profile
    // (Mono + IL2CPP), so no extra dependency is needed.

    // ──────────────────────────────────────────────────────────────────────────
    //  Utf8 — decode a native NUL-terminated UTF-8 char* into a managed string.
    //
    //  Marshal.PtrToStringUTF8 exists on .NET Standard 2.1 (Unity 2021.2+ with the
    //  ".NET Standard 2.1" API Compatibility Level, the modern default). On older
    //  Unity (.NET Standard 2.0 / .NET 4.x without it) we decode manually. The
    //  manual path is always correct, so we use it under the conditional symbol;
    //  define DAIMON_NO_UTF8_MARSHAL if PtrToStringUTF8 is unavailable in your
    //  profile and you hit a compile error on the fast path.
    // ──────────────────────────────────────────────────────────────────────────
    internal static class Utf8
    {
        internal static string FromPtr(IntPtr ptr)
        {
            if (ptr == IntPtr.Zero)
                return null;
#if !DAIMON_NO_UTF8_MARSHAL
            // Fast path: framework-provided UTF-8 decode (no extra free; this does
            // NOT free the native buffer — the caller still owns it).
            return Marshal.PtrToStringUTF8(ptr);
#else
            // Portable fallback for .NET Standard 2.0: copy bytes up to the NUL,
            // then UTF8-decode. Reads one byte at a time so we never over-read.
            int len = 0;
            while (Marshal.ReadByte(ptr, len) != 0)
                len++;
            if (len == 0)
                return string.Empty;
            byte[] bytes = new byte[len];
            Marshal.Copy(ptr, bytes, 0, len);
            return Encoding.UTF8.GetString(bytes);
#endif
        }
    }
}
