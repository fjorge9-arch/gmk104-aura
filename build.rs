fn main() {
    // Ícone do .exe (Explorer) e metadados de versão. Precisa do rc.exe do Windows SDK.
    if std::path::Path::new("assets/icon.ico").exists() {
        let mut res = winres::WindowsResource::new();
        res.set_icon("assets/icon.ico");
        res.set("ProductName", "gmk104-aura");
        res.set("FileDescription", "Teclado GMK104 segue a cor do ASUS Aura Sync");
        res.set("LegalCopyright", "MIT License");
        if let Err(e) = res.compile() {
            println!("cargo:warning=recurso do Windows não compilado ({e}); o exe fica sem ícone");
        }
    }
    println!("cargo:rerun-if-changed=assets/icon.ico");
    println!("cargo:rerun-if-changed=build.rs");
}
