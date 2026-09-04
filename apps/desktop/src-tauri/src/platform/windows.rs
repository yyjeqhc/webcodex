use std::os::windows::process::CommandExt;
use std::process::Stdio;
use std::time::Duration;
use tokio::process::Command;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub fn configure_child(command: &mut Command) {
    command.as_std_mut().creation_flags(CREATE_NO_WINDOW);
}

pub async fn force_stop_owned_tree(pid: u32) -> bool {
    let mut command = Command::new("taskkill.exe");
    command
        .arg("/PID")
        .arg(pid.to_string())
        .arg("/T")
        .arg("/F")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    configure_child(&mut command);
    matches!(
        tokio::time::timeout(Duration::from_secs(5), command.status()).await,
        Ok(Ok(status)) if status.success()
    )
}
