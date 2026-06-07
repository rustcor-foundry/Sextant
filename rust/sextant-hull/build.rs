fn main() {
    println!("cargo:rerun-if-changed=assets/sextant.rc");
    println!("cargo:rerun-if-changed=assets/icons/sextant.ico");

    if std::env::var_os("CARGO_CFG_WINDOWS").is_some() {
        embed_resource::compile("assets/sextant.rc", embed_resource::NONE)
            .manifest_optional()
            .expect("failed to embed Sextant Windows resources");
    }
}
