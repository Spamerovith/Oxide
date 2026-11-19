fn main() {
    println!("cargo:rerun-if-changed=assets/oxide.ico");
    println!("cargo:rerun-if-changed=assets/oxide.manifest");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/oxide.ico");
        res.set_manifest_file("assets/oxide.manifest");
        res.set("FileDescription", "Oxide");
        res.set("ProductName", "Oxide");
        res.set("CompanyName", "Oxide");
        res.set("LegalCopyright", "Oxide");
        if let Err(e) = res.compile() {
            println!("cargo:warning=resource compile failed: {e}");
        }
    }
}
