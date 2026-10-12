fn main() {
    let config = slint_build::CompilerConfiguration::new()
        .with_style("fluent-light".into())
        .with_bundled_translations("lang")
        .with_default_translation_context(slint_build::DefaultTranslationContext::None);
    slint_build::compile_with_config("ui/app.slint", config).expect("failed to compile the UI");

    // icon and file details shown by Explorer for the .exe
    let mut res = winresource::WindowsResource::new();
    res.set_icon("ui/assets/app.ico").set("FileDescription", "A9 Panel").set("ProductName", "A9 Panel");
    res.compile().expect("failed to embed the Windows icon");
}