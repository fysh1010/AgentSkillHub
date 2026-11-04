// Both development and release desktop builds must start without a console window.
#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

fn main() {
    agentskillhub_lib::run()
}
