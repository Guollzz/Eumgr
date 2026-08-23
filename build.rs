fn main() {
    slint_build::compile("components/MainWindow.slint").unwrap();
    
    let mut res = winres::WindowsResource::new();
    res.set_icon("assets/icon/icon.ico");
    res.compile().unwrap();
}