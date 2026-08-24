#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use eumgr::run;

fn main() {
    #[cfg(windows)]
    colored::control::set_virtual_terminal(true).unwrap();

    run();
}
