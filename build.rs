fn main() {
    let config = slint_build::CompilerConfiguration::new()
        .with_bundled_translations("lang");
    slint_build::compile_with_config("components/MainWindow.slint", config).unwrap();

    slint_build::compile("components/MainWindow.slint").unwrap();
    
    let mut res = winres::WindowsResource::new();
    res.set_icon("assets/icon/icon.ico");
    res.compile().unwrap();
}