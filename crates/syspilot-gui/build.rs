fn main() {
    slint_build::compile_with_config(
        "ui/appwindow.slint",
        slint_build::CompilerConfiguration::new().with_edition((1, 19)),
    )
    .unwrap();
    slint_build::compile_with_output_path(
        "ui/patterns/patterns_showcase.slint",
        std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap()).join("patterns_check.rs"),
        slint_build::CompilerConfiguration::new().with_edition((1, 19)),
    )
    .unwrap();
    println!("cargo:rerun-if-changed=ui/patterns");
    println!("cargo:rerun-if-changed=slint-project.json");
}
