// Builds the vendored game-music-emu 0.6.6 (LGPL-2.1, see LICENSES/) into a
// static library, mirroring the configuration of the web edition's wasm build:
// Nuked YM2612 core, no zlib (.vgz is gunzipped in Rust before reaching GME).
fn main() {
    // blargg_endian.h requires the build system to state byte order.
    let endian = match std::env::var("CARGO_CFG_TARGET_ENDIAN").as_deref() {
        Ok("big") => "BLARGG_BIG_ENDIAN",
        _ => "BLARGG_LITTLE_ENDIAN",
    };

    let mut gme = cc::Build::new();
    gme.cpp(true)
        .std("c++11")
        .include("vendor/gme")
        .define("VGM_YM2612_NUKED", None)
        .define(endian, "1")
        .define("NDEBUG", None)
        .warnings(false);

    for entry in glob::glob("vendor/gme/*.cpp").expect("glob vendor/gme") {
        let path = entry.expect("read vendor/gme entry");
        let name = path.file_name().unwrap().to_str().unwrap();
        // The GENS/MAME YM2612 cores are alternatives to Nuked; their sources
        // are also self-guarded by #ifdef, but skip them outright.
        if name == "Ym2612_GENS.cpp" || name == "Ym2612_MAME.cpp" {
            continue;
        }
        gme.file(&path);
    }
    gme.compile("gme");

    // emu2413 (MIT) is plain C, referenced by the KSS and NES VRC7 emulators.
    cc::Build::new()
        .file("vendor/gme/ext/emu2413.c")
        .include("vendor/gme")
        .define("NDEBUG", None)
        .warnings(false)
        .compile("emu2413");

    // Tracker formats (.mod .xm .s3m .it) come from the system libopenmpt.
    println!("cargo:rustc-link-lib=dylib=openmpt");

    println!("cargo:rerun-if-changed=vendor/gme");
}
