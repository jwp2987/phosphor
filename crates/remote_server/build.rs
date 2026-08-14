fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=proto/remote_server.proto");

    // `setup.rs::expected_sha256` reads these with `option_env!`, which bakes the value in at
    // compile time. Cargo does not know an `option_env!` is a build input, so without these
    // directives a crate already in the build cache would keep whatever digests it was first
    // compiled with -- and a release could ship a client pinning the *previous* release's
    // tarball hashes, which fails closed on every remote install and looks like a tampering
    // alert rather than a stale cache. CI builds cold, so this matters most for local builds
    // and for any future cache reuse between release runs.
    for platform in [
        "LINUX_X86_64",
        "LINUX_AARCH64",
        "MACOS_X86_64",
        "MACOS_AARCH64",
    ] {
        println!("cargo:rerun-if-env-changed=PHOSPHOR_CLI_SHA256_{platform}");
    }

    prost_build::compile_protos(&["proto/remote_server.proto"], &["proto/"])?;
    Ok(())
}
