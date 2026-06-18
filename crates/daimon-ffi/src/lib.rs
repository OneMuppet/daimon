//! # daimon-ffi — Daimon for Unity, Unreal, and any C/C++/C# host
//!
//! A C ABI around the Daimon cognitive engine. The engine is pure Rust; this
//! crate compiles to a native library (`.dll` / `.dylib` / `.so` / `.a`) that a
//! non-Rust game can load and call.
//!
//! ## Why JSON, and why *flat* JSON
//!
//! Everything crosses the boundary as a JSON string, so the ABI is tiny and
//! stable (only `char*`). Crucially the JSON is **flat** — plain scalar fields,
//! never Rust's tagged-enum shapes — because C# (`System.Text.Json`,
//! `JsonUtility`) and C++ JSON libraries handle flat objects trivially but choke
//! on tagged unions. You build a small object describing what an NPC senses; you
//! get back a small object describing what it decided.
//!
//! ## The five calls you need
//!
//! - [`daimon_agent_new`] — spawn a mind from a persona JSON + seed → handle.
//! - [`daimon_agent_think`] — feed one tick of perception JSON → decision JSON.
//! - [`daimon_agent_save`] / [`daimon_agent_load`] — persist/restore a mind.
//! - [`daimon_agent_free`] — destroy a handle. [`daimon_string_free`] — free any
//!   string this library returned.
//!
//! Plus [`daimon_version`] and [`daimon_last_error`] for diagnostics.
//!
//! ## Memory ownership (the one rule)
//!
//! Every non-null `char*` this library **returns** is owned by the caller and
//! **must** be released with [`daimon_string_free`]. Every handle from
//! [`daimon_agent_new`]/[`daimon_agent_load`] must be released with
//! [`daimon_agent_free`]. Strings you **pass in** are borrowed and copied; you
//! keep ownership. All entry points are panic-safe (a panic is caught and turned
//! into a null return + [`daimon_last_error`]), so a misbehaving tick can never
//! unwind across the FFI boundary into your engine.
//!
//! ## JSON shapes
//!
//! **Persona in** (`daimon_agent_new`): all fields optional.
//! ```json
//! { "name": "Mara", "boldness": 0.6, "sociability": 0.4, "curiosity": 0.9,
//!   "creed": "I want to understand this place." }
//! ```
//!
//! **Perception in** (`daimon_agent_think`):
//! ```json
//! { "body":   { "x": 5, "y": 5, "health": 1.0, "energy": 0.8, "hydration": 0.7,
//!               "enclosure": 0.0, "season": 0, "winter_in": 1e30, "carrying": 0.0,
//!               "shelter_gap": null, "gather_dir": null, "store_dir": null },
//!   "visible": [ { "id": 2, "kind": "food", "x": 6, "y": 5, "label": "berry" } ],
//!   "events":  [ { "kind": "hurt", "id": 99, "health": 0.2 },
//!                { "kind": "died", "id": 7, "x": 3, "y": 4, "cause": "the stalker" } ] }
//! ```
//! `kind` for visible entities is one of `food|water|agent|predator|curio`.
//! `event.kind` is one of `ate|drank|hurt|repelled|heard|discovered|vanished|died|
//! told` (carrying the fields it needs: `id`, `energy`, `health`, `from`, `text`,
//! `x`, `y`, `cause`). Directions are `north|south|east|west` or `null`.
//!
//! **These are the complete set of inbound events.** `told` is inter-agent
//! information sharing — one NPC telling another something it can *act on* (vs
//! `heard`, which is sentiment only) — and carries an `info` discriminator:
//! `{"kind":"told","from":5,"info":"resource_at","id":2,"entity_kind":"food","x":6,"y":5,"label":"berry"}`,
//! `{"kind":"told","from":5,"info":"danger_at","x":3,"y":4}`, or
//! `{"kind":"told","from":5,"info":"greeting"}`. (The mind's *own* speech event,
//! `Spoke`, is not an input and is intentionally not exposed.)
//!
//! **Decision out** (`daimon_agent_think`):
//! ```json
//! { "action": "move", "dir": "north", "target": null, "pos": null, "text": null,
//!   "goal": "forage", "drive": "hunger", "process": "routine", "inner": "…" }
//! ```
//! `action` is `move|eat|drink|talk|inspect|strike|build|gather|store|rest|wait`.
//! For `move`, read `dir`; for `eat|drink|inspect|strike|talk`, read `target`
//! (an entity id); for `build`, read `pos` (`[x, y]`); for `talk`, also `text`.

#![allow(clippy::missing_safety_doc)]

use daimon_sdk::prelude::*;
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::ffi::{c_char, CStr, CString};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::ptr;

thread_local! {
    static LAST_ERR: RefCell<Option<CString>> = const { RefCell::new(None) };
}

fn set_err(msg: impl Into<String>) {
    let c = CString::new(msg.into()).unwrap_or_else(|_| CString::new("error").unwrap());
    LAST_ERR.with(|e| *e.borrow_mut() = Some(c));
}

/// Borrow an incoming C string as `&str`, or `None` if null/invalid UTF-8.
unsafe fn as_str<'a>(p: *const c_char) -> Option<&'a str> {
    if p.is_null() {
        return None;
    }
    CStr::from_ptr(p).to_str().ok()
}

/// Hand a Rust `String` to the caller as an owned C string (freed via
/// [`daimon_string_free`]). Returns null if the string contained a NUL byte.
fn into_c(s: String) -> *mut c_char {
    match CString::new(s) {
        Ok(c) => c.into_raw(),
        Err(_) => ptr::null_mut(),
    }
}

// ── flat DTOs ────────────────────────────────────────────────────────────────

fn half() -> f32 {
    0.5
}
fn winter_never() -> f32 {
    f32::MAX
}

#[derive(Deserialize)]
struct FfiPersona {
    #[serde(default = "default_name")]
    name: String,
    #[serde(default = "half")]
    boldness: f32,
    #[serde(default = "half")]
    sociability: f32,
    #[serde(default = "half")]
    curiosity: f32,
    #[serde(default)]
    creed: Option<String>,
}
fn default_name() -> String {
    "Agent".into()
}

#[derive(Deserialize)]
struct FfiBody {
    x: i32,
    y: i32,
    #[serde(default = "one")]
    health: f32,
    #[serde(default = "one")]
    energy: f32,
    #[serde(default = "one")]
    hydration: f32,
    #[serde(default)]
    enclosure: f32,
    #[serde(default)]
    shelter_gap: Option<String>,
    #[serde(default)]
    season: u8,
    #[serde(default = "winter_never")]
    winter_in: f32,
    #[serde(default)]
    carrying: f32,
    #[serde(default)]
    gather_dir: Option<String>,
    #[serde(default)]
    store_dir: Option<String>,
}
fn one() -> f32 {
    1.0
}

#[derive(Deserialize)]
struct FfiEntity {
    id: u32,
    kind: String,
    x: i32,
    y: i32,
    #[serde(default)]
    label: String,
}

#[derive(Deserialize)]
struct FfiEvent {
    kind: String,
    #[serde(default)]
    id: u32,
    #[serde(default)]
    from: u32,
    #[serde(default)]
    energy: f32,
    #[serde(default)]
    health: f32,
    #[serde(default)]
    x: i32,
    #[serde(default)]
    y: i32,
    #[serde(default)]
    text: String,
    #[serde(default)]
    cause: String,
    // for `told` (inter-agent information sharing)
    #[serde(default)]
    info: String,
    #[serde(default)]
    entity_kind: String,
    #[serde(default)]
    label: String,
}

#[derive(Deserialize)]
struct FfiPercept {
    body: FfiBody,
    #[serde(default)]
    visible: Vec<FfiEntity>,
    #[serde(default)]
    events: Vec<FfiEvent>,
}

#[derive(Serialize)]
struct FfiThought {
    action: &'static str,
    dir: Option<&'static str>,
    target: Option<u32>,
    pos: Option<[i32; 2]>,
    text: Option<String>,
    goal: String,
    drive: &'static str,
    process: &'static str,
    inner: String,
}

// ── flat ⇆ engine conversions ────────────────────────────────────────────────

fn dir_from_str(s: &str) -> Option<Dir> {
    match s.to_ascii_lowercase().as_str() {
        "north" | "n" => Some(Dir::North),
        "south" | "s" => Some(Dir::South),
        "east" | "e" => Some(Dir::East),
        "west" | "w" => Some(Dir::West),
        _ => None,
    }
}
fn dir_to_str(d: Dir) -> &'static str {
    match d {
        Dir::North => "north",
        Dir::South => "south",
        Dir::East => "east",
        Dir::West => "west",
    }
}
fn opt_dir(s: &Option<String>) -> Option<Dir> {
    s.as_deref().and_then(dir_from_str)
}

fn kind_from_str(s: &str) -> Option<EntityKind> {
    match s.to_ascii_lowercase().as_str() {
        "food" => Some(EntityKind::Food),
        "water" => Some(EntityKind::Water),
        "agent" => Some(EntityKind::Agent),
        "predator" => Some(EntityKind::Predator),
        "curio" => Some(EntityKind::Curio),
        _ => None,
    }
}

impl FfiBody {
    fn into_self_state(self) -> SelfState {
        SelfState {
            pos: Pos::new(self.x, self.y),
            health: self.health,
            energy: self.energy,
            hydration: self.hydration,
            enclosure: self.enclosure,
            shelter_gap: opt_dir(&self.shelter_gap),
            season: self.season,
            winter_in: self.winter_in,
            carrying: self.carrying,
            gather_dir: opt_dir(&self.gather_dir),
            store_dir: opt_dir(&self.store_dir),
        }
    }
}

impl FfiEntity {
    fn into_entity(self) -> Option<Entity> {
        Some(Entity {
            id: EntityId(self.id),
            kind: kind_from_str(&self.kind)?,
            pos: Pos::new(self.x, self.y),
            label: self.label,
        })
    }
}

impl FfiEvent {
    fn into_world_event(self) -> Option<WorldEvent> {
        Some(match self.kind.to_ascii_lowercase().as_str() {
            "ate" => WorldEvent::Ate { id: EntityId(self.id), energy: self.energy },
            "drank" => WorldEvent::Drank { id: EntityId(self.id) },
            "hurt" => WorldEvent::Hurt { id: EntityId(self.id), health: self.health },
            "repelled" => WorldEvent::Repelled { id: EntityId(self.id) },
            "heard" => WorldEvent::Heard { from: EntityId(self.from), text: self.text },
            "discovered" => WorldEvent::Discovered { id: EntityId(self.id) },
            "vanished" => WorldEvent::Vanished { id: EntityId(self.id) },
            "died" => WorldEvent::Died { id: EntityId(self.id), pos: Pos::new(self.x, self.y), cause: self.cause },
            // inter-agent information sharing: one NPC tells another something it
            // can act on (a resource location, a danger zone) — `info` selects the
            // payload. This is what makes "an NPC reacting to words" differ from one
            // merely emoting (cf. `heard`, which is sentiment only).
            "told" => {
                let info = match self.info.to_ascii_lowercase().as_str() {
                    "" | "greeting" => Info::Greeting,
                    "resource_at" => Info::ResourceAt {
                        id: EntityId(self.id),
                        kind: kind_from_str(&self.entity_kind)?,
                        pos: Pos::new(self.x, self.y),
                        label: self.label,
                    },
                    "danger_at" => Info::DangerAt { pos: Pos::new(self.x, self.y) },
                    _ => return None,
                };
                WorldEvent::Told { from: EntityId(self.from), info }
            }
            _ => return None, // unknown event kinds are ignored, not fatal
        })
    }
}

fn process_str(p: Process) -> &'static str {
    match p {
        Process::Reflex => "reflex",
        Process::Routine => "routine",
        Process::Deliberate => "deliberate",
    }
}

fn flatten_thought(t: &Thought) -> FfiThought {
    let mut out = FfiThought {
        action: t.action.verb(),
        dir: None,
        target: None,
        pos: None,
        text: None,
        goal: t.goal.label(),
        drive: t.dominant_drive.name(),
        process: process_str(t.process),
        inner: t.inner.clone(),
    };
    match &t.action {
        Action::Move(d) => out.dir = Some(dir_to_str(*d)),
        Action::Eat(id) | Action::Drink(id) | Action::Inspect(id) | Action::Strike(id) => {
            out.target = Some(id.0)
        }
        Action::Talk { to, text } => {
            out.target = Some(to.0);
            out.text = Some(text.clone());
        }
        Action::Build(p) => out.pos = Some([p.x, p.y]),
        Action::Gather | Action::Store | Action::Rest | Action::Wait => {}
    }
    out
}

// ── the C ABI ─────────────────────────────────────────────────────────────────

/// Spawn an agent from a persona JSON (see crate docs; all fields optional) and a
/// deterministic `seed`. Returns an opaque handle, or null on error (see
/// [`daimon_last_error`]). Free it with [`daimon_agent_free`].
///
/// **Give each NPC a distinct seed** — two agents sharing a seed behave
/// identically the moment they share a percept.
#[no_mangle]
pub unsafe extern "C" fn daimon_agent_new(persona_json: *const c_char, seed: u64) -> *mut Agent {
    catch_unwind(AssertUnwindSafe(|| {
        let json = match unsafe { as_str(persona_json) } {
            Some(s) => s,
            None => {
                set_err("persona_json is null or not valid UTF-8");
                return ptr::null_mut();
            }
        };
        let fp: FfiPersona = match serde_json::from_str(json) {
            Ok(p) => p,
            Err(e) => {
                set_err(format!("bad persona JSON: {e}"));
                return ptr::null_mut();
            }
        };
        let mut persona = Persona::new(&fp.name)
            .with_boldness(fp.boldness)
            .with_sociability(fp.sociability)
            .with_curiosity(fp.curiosity);
        if let Some(creed) = fp.creed {
            persona = persona.with_creed(&creed);
        }
        Box::into_raw(Box::new(Agent::new(EntityId(0), persona, seed)))
    }))
    .unwrap_or_else(|_| {
        set_err("panic in daimon_agent_new");
        ptr::null_mut()
    })
}

/// Advance the mind one tick. `input_json` is the flat perception object (see
/// crate docs). Returns a newly-allocated decision JSON string (free with
/// [`daimon_string_free`]), or null on error.
#[no_mangle]
pub unsafe extern "C" fn daimon_agent_think(agent: *mut Agent, input_json: *const c_char) -> *mut c_char {
    catch_unwind(AssertUnwindSafe(|| {
        let agent = match unsafe { agent.as_mut() } {
            Some(a) => a,
            None => {
                set_err("agent handle is null");
                return ptr::null_mut();
            }
        };
        let json = match unsafe { as_str(input_json) } {
            Some(s) => s,
            None => {
                set_err("input_json is null or not valid UTF-8");
                return ptr::null_mut();
            }
        };
        let p: FfiPercept = match serde_json::from_str(json) {
            Ok(p) => p,
            Err(e) => {
                set_err(format!("bad perception JSON: {e}"));
                return ptr::null_mut();
            }
        };
        // world-driven events that happened to this NPC, delivered this tick
        for ev in p.events {
            if let Some(we) = ev.into_world_event() {
                agent.observe(we);
            }
        }
        let visible: Vec<Entity> = p.visible.into_iter().filter_map(FfiEntity::into_entity).collect();
        let thought = agent.think(p.body.into_self_state(), visible);
        match serde_json::to_string(&flatten_thought(&thought)) {
            Ok(s) => into_c(s),
            Err(e) => {
                set_err(format!("could not serialise decision: {e}"));
                ptr::null_mut()
            }
        }
    }))
    .unwrap_or_else(|_| {
        set_err("panic in daimon_agent_think");
        ptr::null_mut()
    })
}

/// Serialise the mind to a JSON string for save games (free with
/// [`daimon_string_free`]). Null on error.
#[no_mangle]
pub unsafe extern "C" fn daimon_agent_save(agent: *mut Agent) -> *mut c_char {
    catch_unwind(AssertUnwindSafe(|| match unsafe { agent.as_ref() } {
        Some(a) => into_c(a.save()),
        None => {
            set_err("agent handle is null");
            ptr::null_mut()
        }
    }))
    .unwrap_or_else(|_| {
        set_err("panic in daimon_agent_save");
        ptr::null_mut()
    })
}

/// Restore a mind from a [`daimon_agent_save`] JSON string. `name` labels the
/// restored agent. Returns a handle (free with [`daimon_agent_free`]) or null.
#[no_mangle]
pub unsafe extern "C" fn daimon_agent_load(name: *const c_char, mind_json: *const c_char) -> *mut Agent {
    catch_unwind(AssertUnwindSafe(|| {
        let name = unsafe { as_str(name) }.unwrap_or("Agent");
        let json = match unsafe { as_str(mind_json) } {
            Some(s) => s,
            None => {
                set_err("mind_json is null or not valid UTF-8");
                return ptr::null_mut();
            }
        };
        match Agent::load(EntityId(0), name, json) {
            Some(a) => Box::into_raw(Box::new(a)),
            None => {
                set_err("mind_json is not a valid saved mind");
                ptr::null_mut()
            }
        }
    }))
    .unwrap_or_else(|_| {
        set_err("panic in daimon_agent_load");
        ptr::null_mut()
    })
}

/// Destroy an agent handle. Safe to call with null (no-op). Do not use the
/// handle afterwards.
#[no_mangle]
pub unsafe extern "C" fn daimon_agent_free(agent: *mut Agent) {
    if !agent.is_null() {
        unsafe { drop(Box::from_raw(agent)) };
    }
}

/// Free a string previously returned by this library. Safe with null.
#[no_mangle]
pub unsafe extern "C" fn daimon_string_free(s: *mut c_char) {
    if !s.is_null() {
        unsafe { drop(CString::from_raw(s)) };
    }
}

/// The library version (the crate version). Static — do **not** free it.
#[no_mangle]
pub extern "C" fn daimon_version() -> *const c_char {
    // a static, NUL-terminated string with 'static lifetime
    concat!(env!("CARGO_PKG_VERSION"), "\0").as_ptr() as *const c_char
}

/// The last error message on this thread (free with [`daimon_string_free`]), or
/// null if there has been none. Calling it clears the stored error.
#[no_mangle]
pub extern "C" fn daimon_last_error() -> *mut c_char {
    LAST_ERR.with(|e| match e.borrow_mut().take() {
        Some(c) => c.into_raw(),
        None => ptr::null_mut(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(s: &str) -> CString {
        CString::new(s).unwrap()
    }
    unsafe fn take(p: *mut c_char) -> String {
        assert!(!p.is_null(), "expected a string, got null (err: {:?})", err());
        let s = CStr::from_ptr(p).to_str().unwrap().to_owned();
        daimon_string_free(p);
        s
    }
    fn err() -> Option<String> {
        let p = daimon_last_error();
        if p.is_null() {
            None
        } else {
            unsafe { Some(take(p)) }
        }
    }

    #[test]
    fn full_round_trip_through_the_c_abi() {
        unsafe {
            let persona = c(r#"{"name":"Mara","curiosity":0.9}"#);
            let agent = daimon_agent_new(persona.as_ptr(), 42);
            assert!(!agent.is_null(), "new failed: {:?}", err());

            let input = c(r#"{"body":{"x":5,"y":5,"energy":0.3},
                "visible":[{"id":2,"kind":"food","x":6,"y":5,"label":"berry"}],
                "events":[{"kind":"hurt","id":99,"health":0.1}]}"#);
            let out = take(daimon_agent_think(agent, input.as_ptr()));
            // flat, language-agnostic decision JSON
            assert!(out.contains("\"action\""), "missing action: {out}");
            assert!(out.contains("\"inner\""), "missing inner: {out}");
            let v: serde_json::Value = serde_json::from_str(&out).unwrap();
            assert!(v["action"].is_string());
            assert!(v["drive"].is_string());

            // save → load → think again
            let saved = take(daimon_agent_save(agent));
            let name = c("Mara");
            let saved_c = c(&saved);
            let restored = daimon_agent_load(name.as_ptr(), saved_c.as_ptr());
            assert!(!restored.is_null(), "load failed: {:?}", err());

            daimon_agent_free(agent);
            daimon_agent_free(restored);
        }
    }

    #[test]
    fn deterministic_across_two_fresh_agents() {
        let run = || unsafe {
            let persona = c(r#"{"name":"Echo","curiosity":0.8}"#);
            let a = daimon_agent_new(persona.as_ptr(), 7);
            let mut acts = Vec::new();
            for _ in 0..15 {
                let input = c(r#"{"body":{"x":5,"y":5}}"#);
                let out = take(daimon_agent_think(a, input.as_ptr()));
                let v: serde_json::Value = serde_json::from_str(&out).unwrap();
                acts.push(v["action"].as_str().unwrap().to_owned());
            }
            daimon_agent_free(a);
            acts
        };
        assert_eq!(run(), run(), "same seed + same percepts must reproduce");
    }

    #[test]
    fn told_event_is_accepted_through_the_abi() {
        // inter-agent info sharing must survive the flat-JSON boundary
        unsafe {
            let agent = daimon_agent_new(c(r#"{"name":"Mira","sociability":0.9}"#).as_ptr(), 3);
            assert!(!agent.is_null());
            let input = c(r#"{"body":{"x":4,"y":4},
                "events":[{"kind":"told","from":2,"info":"resource_at","id":7,"entity_kind":"water","x":9,"y":1,"label":"spring"},
                          {"kind":"told","from":2,"info":"danger_at","x":3,"y":3}]}"#);
            let out = take(daimon_agent_think(agent, input.as_ptr()));
            assert!(out.contains("\"action\""), "told input should still produce a decision: {out}");
            assert!(err().is_none(), "told events must not error: {:?}", err());
            daimon_agent_free(agent);
        }
    }

    #[test]
    fn told_resource_maps_to_exact_info_payload() {
        // Guards the flat-JSON field names against silent drift: if `entity_kind`
        // (or any field) were renamed on either side, this fails in CI rather than
        // a resource-share silently degrading to a greeting / being dropped.
        let ev: FfiEvent = serde_json::from_str(
            r#"{"kind":"told","from":2,"info":"resource_at","id":7,"entity_kind":"water","x":9,"y":1,"label":"spring"}"#,
        )
        .unwrap();
        match ev.into_world_event() {
            Some(WorldEvent::Told { from, info: Info::ResourceAt { id, kind, pos, label } }) => {
                assert_eq!(from, EntityId(2));
                assert_eq!(id, EntityId(7));
                assert_eq!(kind, EntityKind::Water);
                assert_eq!(pos, Pos::new(9, 1));
                assert_eq!(label, "spring");
            }
            other => panic!("told/resource_at must map to Info::ResourceAt, got {other:?}"),
        }

        // danger_at carries only a position
        let danger: FfiEvent =
            serde_json::from_str(r#"{"kind":"told","from":3,"info":"danger_at","x":4,"y":5}"#).unwrap();
        assert!(matches!(
            danger.into_world_event(),
            Some(WorldEvent::Told { info: Info::DangerAt { pos }, .. }) if pos == Pos::new(4, 5)
        ));
    }

    #[test]
    fn bad_input_sets_error_not_panic() {
        unsafe {
            let agent = daimon_agent_new(c("{}").as_ptr(), 1);
            assert!(!agent.is_null());
            let bad = daimon_agent_think(agent, c("not json").as_ptr());
            assert!(bad.is_null(), "garbage input should return null");
            assert!(err().is_some(), "a diagnostic should be set");
            daimon_agent_free(agent);
        }
    }

    #[test]
    fn null_handles_are_safe() {
        unsafe {
            assert!(daimon_agent_think(ptr::null_mut(), c("{}").as_ptr()).is_null());
            daimon_agent_free(ptr::null_mut()); // no-op, must not crash
            daimon_string_free(ptr::null_mut()); // no-op, must not crash
        }
    }
}
