//! Embeds the exe icon and version info when targeting Windows.

fn main() {
    println!("cargo:rerun-if-changed=res/icon.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        winresource::WindowsResource::new()
            .set_icon("res/icon.ico")
            .compile()
            .expect("embed Windows resources");
    }
}
