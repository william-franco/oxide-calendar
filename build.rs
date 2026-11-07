use std::collections::HashMap;
use std::path::PathBuf;

fn main() {
    let config = slint_build::CompilerConfiguration::new()
        .with_style("material".into())
        .with_library_paths(HashMap::from([(
            oxide_ui::LIBRARY_NAME.to_string(),
            PathBuf::from(oxide_ui::SLINT_DIR),
        )]));

    slint_build::compile_with_config("ui/app.slint", config).expect("failed to compile Slint UI");
}
