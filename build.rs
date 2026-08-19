fn main() {
    println!("cargo:rerun-if-changed=assets/sabun.ico");

    #[cfg(windows)]
    winresource::WindowsResource::new()
        .set_icon("assets/sabun.ico")
        .compile()
        .expect("failed to embed the sabun application icon");
}
