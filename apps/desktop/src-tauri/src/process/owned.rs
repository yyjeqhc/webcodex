use crate::platform;
use std::time::Duration;
use tokio::process::Child;
use tokio::time::Instant;

pub(crate) async fn reclaim_owned_tree(
    child: &mut Child,
    tree: platform::OwnedProcessTree,
    deadline: Instant,
) {
    let _ = platform::terminate_owned_tree(tree).await;
    if !wait_for_owned_tree_stop(child, tree, deadline).await {
        let _ = platform::force_stop_owned_tree(tree).await;
        let _ = child.start_kill();
        let _ = wait_for_owned_tree_stop(child, tree, deadline).await;
    }
}

#[cfg(target_os = "macos")]
async fn wait_for_owned_tree_stop(
    child: &mut Child,
    tree: platform::OwnedProcessTree,
    deadline: Instant,
) -> bool {
    loop {
        let _ = child.try_wait();
        if !platform::owned_tree_is_running(tree) {
            return true;
        }
        let now = Instant::now();
        if now >= deadline {
            return false;
        }
        tokio::time::sleep(std::cmp::min(Duration::from_millis(25), deadline - now)).await;
    }
}

#[cfg(not(target_os = "macos"))]
async fn wait_for_owned_tree_stop(
    child: &mut Child,
    _tree: platform::OwnedProcessTree,
    deadline: Instant,
) -> bool {
    let now = Instant::now();
    if now >= deadline {
        return child.try_wait().ok().flatten().is_some();
    }
    matches!(
        tokio::time::timeout_at(deadline, child.wait()).await,
        Ok(Ok(_))
    )
}
