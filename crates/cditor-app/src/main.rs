#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

use cditor_api::Cditor;

fn cditor_from_env() -> Cditor {
    Cditor::new().demo()
}

fn main() {
    cditor_app::wiring::run_desktop(cditor_from_env());
}
