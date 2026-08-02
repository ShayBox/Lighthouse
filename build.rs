use std::path::Path;

use cbindgen::Config;

fn main() {
    println!("cargo:rerun-if-changed=src/lib.rs");
    println!("cargo:rerun-if-changed=src/ffi.rs");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=cbindgen.toml");

    if std::env::var("CARGO_FEATURE_FFI").is_ok() {
        generate_header();
    }
}

fn generate_header() {
    let crate_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let config = Config::from_root_or_default(&crate_dir);
    let header = match cbindgen::Builder::new()
        .with_config(config)
        .with_crate(&crate_dir)
        .generate()
    {
        Ok(h) => h,
        Err(e) => {
            println!("cargo:warning=cbindgen generation failed: {e}");
            return;
        }
    };

    let include_path = Path::new(&crate_dir).join("include").join("lighthouse.h");
    if let Some(parent) = include_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    header.write_to_file(&include_path);
}
