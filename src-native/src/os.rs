pub fn reveal(path: &std::path::Path, select: bool) -> zenv_core::error::AppResult<()> {
    use zenv_core::error::AppError;
    if !path.exists() {
        return Err(AppError::invalid_path(path, "does not exist yet"));
    }
    let mut cmd = std::process::Command::new("explorer");
    if select {
        let mut arg = std::ffi::OsString::from("/select,");
        arg.push(path);
        cmd.arg(arg);
    } else {
        cmd.arg(path);
    }
    cmd.spawn().map(|_| ()).map_err(|e| AppError::Platform(format!("cannot open {}: {e}", path.display())))
}
