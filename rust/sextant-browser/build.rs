use std::env;

fn main() {
    // Ship the GUI binaries on the Windows subsystem so no console window pops
    // up — for the app itself and for the direct/Servo child processes it
    // spawns (same exe). Done via a link arg rather than
    // `#![windows_subsystem = "windows"]` because the lite alias `include!`s
    // main.rs, where a crate inner-attribute would be misplaced. `-bins`
    // applies to every binary target in this crate.
    //
    // /SUBSYSTEM:WINDOWS alone would make the linker look for WinMain; pairing
    // it with /ENTRY:mainCRTStartup keeps Rust's `fn main` as the entry point.
    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows")
        && env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc")
    {
        println!("cargo:rustc-link-arg-bins=/SUBSYSTEM:WINDOWS");
        println!("cargo:rustc-link-arg-bins=/ENTRY:mainCRTStartup");
    }
}
