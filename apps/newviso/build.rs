fn main() {
    println!("cargo:rerun-if-changed=north_star_logo.ico");

    if std::env::var_os("CARGO_CFG_WINDOWS").is_none() {
        return;
    }

    let version = std::env::var("CARGO_PKG_VERSION").unwrap_or_else(|_| "0.0.0".to_owned());

    let mut resource = winresource::WindowsResource::new();
    resource.set_icon("north_star_logo.ico");
    resource.set("CompanyName", "Kaylas Systems");
    resource.set("FileDescription", "NewViso");
    resource.set("FileVersion", &version);
    resource.set("InternalName", "NewViso");
    resource.set("LegalCopyright", "Copyright (C) 2026 Take Some");
    resource.set("OriginalFilename", "newviso.exe");
    resource.set("ProductName", "NewViso");
    resource.set("ProductVersion", &version);

    resource
        .compile()
        .expect("failed to compile NewViso Windows resources");
}
