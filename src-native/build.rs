fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let version = std::env::var("CARGO_PKG_VERSION").unwrap();
    let mut parts: Vec<&str> = version.split(['.', '-']).take(3).collect();
    parts.resize(3, "0");
    let defines = [format!("ZENV_VERSION={version}"), format!("ZENV_VERSION_BIN={},0", parts.join(","))];
    println!("cargo:rerun-if-changed=windows");
    embed_resource::compile_for("windows/zenv.rc", ["zenv"], &defines).manifest_optional().unwrap();
}
