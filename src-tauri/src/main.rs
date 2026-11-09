// Release builds run without a console window.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // `zenv.exe --zenv-apply-hosts <request>` is the elevated hosts-file
    // helper launched through UAC; it never starts the GUI.
    if let Some(code) = zenv_core::domain_manager::hosts::run_helper_from_args() {
        std::process::exit(code);
    }
    zenv_lib::run()
}
