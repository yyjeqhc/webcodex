#![forbid(unsafe_code)]
mod config;
mod guard;
mod health_http;

use config::{Action, Config};
use std::time::Duration;
use tokio::sync::oneshot;
use webcodex_openai_tunnel::HealthSnapshot;

#[tokio::main]
async fn main() -> std::process::ExitCode {
    match execute().await {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("webcodex-tunnel: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}
async fn execute() -> Result<(), &'static str> {
    let config = Config::parse(std::env::args().skip(1))?;
    match config.action {
        Action::Help => {
            print!("{}", config::HELP);
            return Ok(());
        }
        Action::Version => {
            println!("webcodex-tunnel {}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        Action::BuildInfo => {
            println!(
                "{}",
                serde_json::json!({
                    "schema_version":1, "binary":"webcodex-tunnel", "version":env!("CARGO_PKG_VERSION"),
                    "target":env!("WEBCODEX_TUNNEL_TARGET"),
                    "source_commit":option_env!("WEBCODEX_TUNNEL_SOURCE_COMMIT"),
                })
            );
            return Ok(());
        }
        Action::Status => {
            let snapshot = health_http::status(config.listen).await?;
            report("status", &snapshot, config.json);
            return if snapshot.ready {
                Ok(())
            } else {
                Err("Tunnel is running but polling is not ready")
            };
        }
        _ => {}
    }
    let client = config.client()?;
    if matches!(config.action, Action::Doctor) {
        if config.json {
            println!("{{\"event\":\"configuration_valid\",\"network_checked\":false}}");
        } else {
            println!("Configuration valid; no network requests or state changes made.");
        }
        return Ok(());
    }
    let listener = tokio::net::TcpListener::bind(config.listen)
        .await
        .map_err(|_| "health port unavailable; choose another --health.listen-addr")?;
    let address = listener
        .local_addr()
        .map_err(|_| "health listener unavailable")?;
    // Register OS handlers before the first poll, avoiding a default termination
    // while startup is waiting on the network.
    #[cfg(unix)]
    let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        .map_err(|_| "SIGTERM handler unavailable")?;
    #[cfg(unix)]
    let mut interrupt = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())
        .map_err(|_| "SIGINT handler unavailable")?;
    let guard = guard::Guard::acquire(&config)?;
    let health = client.health();
    let (stop, stopped) = oneshot::channel();
    let mut stop = Some(stop);
    let run = client.run(async {
        let _ = stopped.await;
    });
    tokio::pin!(run);
    let http = health_http::serve(listener, health.clone());
    tokio::pin!(http);
    let signal = async {
        #[cfg(unix)]
        tokio::select! { _ = terminate.recv() => {}, _ = interrupt.recv() => {} }
        #[cfg(not(unix))]
        {
            // A missing console is not a stop signal in a Windows parent-owned
            // launch. Stdin EOF or the process supervisor still owns shutdown.
            if tokio::signal::ctrl_c().await.is_err() {
                std::future::pending::<()>().await;
            }
        }
    };
    tokio::pin!(signal);
    let eof = stdin_eof(config.stdin_eof);
    tokio::pin!(eof);
    if config.json {
        println!(
            "{}",
            serde_json::json!({"event":"started","pid":std::process::id(),"health_address":address.to_string()})
        );
    } else {
        println!("webcodex-tunnel started; health: http://{address}/health");
    }
    let mut interval = tokio::time::interval(Duration::from_secs(1));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut previous = None;
    let mut health_failed = false;
    let result = loop {
        tokio::select! {
            biased;
            _ = &mut signal => { if let Some(tx)=stop.take() { let _=tx.send(()); } break run.await; }
            _ = &mut eof => { if let Some(tx)=stop.take() { let _=tx.send(()); } break run.await; }
            result = &mut run => break result,
            _ = &mut http => {
                health_failed = true;
                if let Some(tx)=stop.take() { let _=tx.send(()); }
                break run.await;
            }
            _ = interval.tick() => {
                let snapshot = health.snapshot();
                if previous.as_ref()!=Some(&snapshot) { report("health",&snapshot,config.json); previous=Some(snapshot); }
            }
        }
    };
    report("stopped", &health.snapshot(), config.json);
    guard.finish(
        health.has_uncertain_work() || result == Err(webcodex_openai_tunnel::Error::Uncertain),
    )?;
    if health_failed {
        return Err("health listener stopped unexpectedly");
    }
    result.map_err(|error| match error {
        webcodex_openai_tunnel::Error::Authentication => "control-plane authorization rejected",
        webcodex_openai_tunnel::Error::Capacity => {
            "Tunnel capacity exhausted; inspect limits before restarting"
        }
        _ => "Tunnel stopped after a transport or protocol failure",
    })
}
fn report(event: &str, snapshot: &HealthSnapshot, json: bool) {
    if json {
        println!("{}", serde_json::json!({"event":event,"health":snapshot}));
    } else {
        println!(
            "{event}: poll_ready={} unsettled={} rejected={}",
            snapshot.ready, snapshot.unsettled_commands, snapshot.rejected_commands
        );
    }
}
async fn stdin_eof(enabled: bool) {
    if !enabled {
        std::future::pending::<()>().await;
        return;
    }
    let (tx, rx) = oneshot::channel();
    // This thread is only created for a parent explicitly choosing stdin
    // ownership. Process exit terminates it if a signal wins while stdin is open.
    let spawned = std::thread::Builder::new()
        .name("tunnel-stdin".into())
        .spawn(move || {
            use std::io::Read;
            let mut input = std::io::stdin().lock();
            let mut buffer = [0; 256];
            while input.read(&mut buffer).is_ok_and(|n| n != 0) {}
            let _ = tx.send(());
        });
    if spawned.is_ok() {
        let _ = rx.await;
    }
}
