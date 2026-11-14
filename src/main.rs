//! Process entry point.
//!
//! Everything interesting happens in [`eumgr::run`]; this file only sets up the
//! Windows specific process details (GUI subsystem for release builds).
//!
//! The coloured console output is enabled inside [`eumgr::run`], because that
//! happens after the log file exists and can therefore be logged.

// Release builds are pure GUI applications: without the subsystem switch a
// console window would pop up next to the main window. Debug builds keep the
// console so the log output is visible while developing.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use eumgr::run;

fn main() {
    run();
}
