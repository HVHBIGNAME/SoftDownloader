fn main() {
    println!("cargo:rerun-if-changed=assets/SoftDownloader.ico");
    println!("cargo:rerun-if-changed=assets/SoftDownloader.rc");
    println!("cargo:rerun-if-env-changed=SOFTDOWNLOADER_CATALOG_URL");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        embed_resource::compile("assets/SoftDownloader.rc", embed_resource::NONE)
            .manifest_required()
            .expect("Failed to embed the application icon");
    }
}
