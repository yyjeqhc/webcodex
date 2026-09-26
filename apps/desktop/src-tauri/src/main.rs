#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let mut args = std::env::args_os().skip(1);
    if args.next().as_deref() == Some(std::ffi::OsStr::new("--build-info-json"))
        && args.next().is_none()
    {
        let info = webcodex_desktop_lib::desktop_build_info();
        match serde_json::to_string(&info) {
            Ok(json) => println!("{json}"),
            Err(_) => std::process::exit(1),
        }
        return;
    }
    webcodex_desktop_lib::run();
}
