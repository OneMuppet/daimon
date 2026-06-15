// Native entry point for Daimon: Smallworld.
// The web entry lives in lib.rs behind cfg(target_arch = "wasm32").

fn main() {
    daimon_game::run();
}
