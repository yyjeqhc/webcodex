// Desktop owns background runtime processes. Sharing a launcher's console in
// dev/dogfood builds lets closing that console bypass the tray shutdown path.
#![cfg_attr(windows, windows_subsystem = "windows")]

fn main() {
    let mut args = std::env::args_os().skip(1);
    let first = args.next();
    #[cfg(windows)]
    if first.as_deref() == Some(std::ffi::OsStr::new("--user-task")) {
        let args = args
            .map(|arg| arg.into_string())
            .collect::<Result<Vec<_>, _>>();
        let code = args
            .ok()
            .and_then(|args| webcodex_environment::service::run_desktop_user_task(&args).ok())
            .unwrap_or(1);
        std::process::exit(code);
    }
    if first.as_deref() == Some(std::ffi::OsStr::new("--build-info-json")) && args.next().is_none()
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
