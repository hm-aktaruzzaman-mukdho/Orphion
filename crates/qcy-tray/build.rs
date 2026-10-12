//! Embeds the icon and version information into the Windows executable.

fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        println!("cargo:rerun-if-changed=windows.rc");
        println!("cargo:rerun-if-changed=../../assets/qcy-earbuds.ico");
        embed_resource::compile("windows.rc", embed_resource::NONE)
            .manifest_optional()
            .expect("compiling Windows resources");
    }
}
