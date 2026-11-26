use std::collections::HashMap;
use std::path::Path;

fn main() {
    let manifest_dir = std::env::var_os("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR set");
    let material_path = Path::new(&manifest_dir).join("../../material-1.1.0/material.slint");
    println!("cargo:rerun-if-changed=ui");
    println!("cargo:rerun-if-changed=../../assets/fonts");
    println!("cargo:rerun-if-changed=../../assets/icons");

    let config = slint_build::CompilerConfiguration::new()
        .with_library_paths(HashMap::from([("material".to_string(), material_path)]));

    slint_build::compile_with_config("ui/main.slint", config).expect("Slint compilation failed");
}
