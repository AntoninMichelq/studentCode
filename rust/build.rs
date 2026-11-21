// Sous Windows : intègre l'icône (packaging/icon.ico) et le nom du programme au .exe, pour
// qu'ils apparaissent dans l'explorateur, la barre des tâches et les raccourcis.
fn main() {
    println!("cargo:rerun-if-changed=packaging/icon.ico");
    #[cfg(windows)]
    {
        if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
            let mut res = winresource::WindowsResource::new();
            res.set_icon("packaging/icon.ico");
            res.set("ProductName", "SCode");
            res.set("FileDescription", "SCode");
            if let Err(e) = res.compile() {
                println!("cargo:warning=icône Windows non intégrée : {e}");
            }
        }
    }
}
