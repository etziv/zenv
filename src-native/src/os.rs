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

pub fn instance_name(root: &std::path::Path) -> String {
    let path = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    let text = path.display().to_string().to_lowercase().replace('\\', "/");
    format!("Local\\zenv-native-{}", text.trim_start_matches("//?/").trim_end_matches('/'))
}
