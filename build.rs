fn main() {
    println!("cargo:rerun-if-changed=assets/SoftDownloader.ico");
    println!("cargo:rerun-if-changed=assets/SoftDownloader.rc");
    #[cfg(windows)]
    if let embed_resource::CompilationResult::Failed(error) =
        embed_resource::compile("assets/SoftDownloader.rc", embed_resource::NONE)
    {
        panic!("Failed to embed the application icon: {error}");
    }
}

#[cfg(not(windows))]
const _: () = ();
