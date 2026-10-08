fn main() {
    let cfg = || slint_build::CompilerConfiguration::new().with_style("fluent".into());
    // 主窗口：生成 app.rs，并通过 SLINT_INCLUDE_GENERATED 暴露给 include_modules!()
    slint_build::compile_with_config("ui/app.slint", cfg()).expect("slint build failed: app.slint");
    // 阅读窗口：单独生成 reader_win.rs，用显式 include! 引入
    slint_build::compile_with_config("ui/reader_win.slint", cfg())
        .expect("slint build failed: reader_win.slint");
}
