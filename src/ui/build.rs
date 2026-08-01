fn main() {
    let config = slint_build::CompilerConfiguration::new().with_bundled_translations("lang");
    slint_build::compile_with_config("ui/AppWindow.slint", config).unwrap();

    let mut build = cc::Build::new();
    build.cpp(true).file("src/drag_qt.cpp");

    // Qt6Widgets (for QWidget access in the FFI shim).
    if let Ok(lib) = pkg_config::probe_library("Qt6Widgets") {
        for path in lib.include_paths {
            build.include(path);
        }
    }

    build.compile("drag_qt");
}
