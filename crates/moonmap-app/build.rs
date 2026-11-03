//! Embeds Info.plist in the executable, so CoreLocation finds Moonmap's
//! usage descriptions even when it runs unbundled from `cargo run`.

fn main() {
    println!("cargo:rerun-if-changed=Info.plist");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        let plist = concat!(env!("CARGO_MANIFEST_DIR"), "/Info.plist");
        println!("cargo:rustc-link-arg-bin=moonmap=-Wl,-sectcreate,__TEXT,__info_plist,{plist}");
    }
}
