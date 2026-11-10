fn main() {
    println!("cargo:rerun-if-changed=assets/pipe-gui.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    embed_windows_icon();
}

#[cfg(windows)]
fn embed_windows_icon() {
    winres::WindowsResource::new()
        .set_icon("assets/pipe-gui.ico")
        .compile()
        .expect("embed assets/pipe-gui.ico as the Windows exe icon");
}

#[cfg(not(windows))]
fn embed_windows_icon() {}
