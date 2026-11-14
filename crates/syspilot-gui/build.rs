fn main() {
    slint_build::compile("ui/appwindow.slint").unwrap();
    slint_build::compile_with_output_path(
        "ui/patterns/patterns_showcase.slint",
        std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap()).join("patterns_check.rs"),
        slint_build::CompilerConfiguration::new(),
    )
    .unwrap();
    println!("cargo:rerun-if-changed=ui/patterns");
}
