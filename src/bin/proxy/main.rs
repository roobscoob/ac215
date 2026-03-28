mod api;
mod config;
mod db;
mod local_db;

use std::ffi::OsString;
use std::sync::{Arc, Mutex};

use tokio::sync::Mutex as AsyncMutex;

use log::{error, info};

use ac215::crypto::Cipher;
use ac215::packet::header::ChecksumMode;
use ac215::proxy::Proxy;
use ac215::proxy::handlers::nack::NackHandler;
use ac215::proxy::handlers::{EventsHandler, LoggingHandler};
use ac215::proxy::pipeline::FrameHandler;

use config::Config;
use local_db::LocalDb;

const SERVICE_NAME: &str = "Ac215Proxy";

#[derive(Clone)]
pub struct AppState {
    pub handle: ac215::proxy::InterceptorHandle,
    pub nack_handler: Arc<Mutex<NackHandler>>,
    pub events_handler: Arc<Mutex<EventsHandler>>,
    pub local_db: Arc<LocalDb>,
    pub db: Arc<AsyncMutex<db::DbClient>>,
}

fn main() {
    env_logger::init();

    if std::env::args().any(|a| a == "--service") {
        // Launched by the SCM — hand off to the service dispatcher.
        windows_service::service_dispatcher::start(SERVICE_NAME, ffi_service_main)
            .expect("failed to start service dispatcher");
    } else {
        // Running directly (development / debugging).
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("failed to build tokio runtime");
        rt.block_on(run(None));
    }
}

windows_service::define_windows_service!(ffi_service_main, service_main);

fn service_main(_args: Vec<OsString>) {
    use windows_service::service::*;
    use windows_service::service_control_handler::{self, ServiceControlHandlerResult};

    // Register the service control handler.
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    let shutdown_tx = Arc::new(Mutex::new(Some(shutdown_tx)));

    let event_handler = move |control_event| -> ServiceControlHandlerResult {
        match control_event {
            ServiceControl::Stop => {
                if let Some(tx) = shutdown_tx.lock().unwrap().take() {
                    let _ = tx.send(());
                }
                ServiceControlHandlerResult::NoError
            }
            ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
            _ => ServiceControlHandlerResult::NotImplemented,
        }
    };

    let status_handle = service_control_handler::register(SERVICE_NAME, event_handler)
        .expect("failed to register service control handler");

    // Tell the SCM we're starting.
    let _ = status_handle.set_service_status(ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: ServiceState::StartPending,
        controls_accepted: ServiceControlAccept::empty(),
        exit_code: ServiceExitCode::Win32(0),
        checkpoint: 0,
        wait_hint: std::time::Duration::from_secs(30),
        process_id: None,
    });

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("failed to build tokio runtime");

    rt.block_on(async {
        run(Some((status_handle, shutdown_rx))).await;
    });
}

/// Core proxy logic. If `service` is provided, we report SERVICE_RUNNING
/// after the listener is up, and shut down on the stop signal.
async fn run(
    service: Option<(
        windows_service::service_control_handler::ServiceStatusHandle,
        tokio::sync::oneshot::Receiver<()>,
    )>,
) {
    let config_path = std::env::args()
        .find(|a| a != "--service" && !a.contains("proxy"))
        .unwrap_or_else(|| "proxy.toml".to_string());

    let config = Config::load(&config_path);

    let local_db = Arc::new(
        LocalDb::open(&config.local_database.path).expect("failed to open local database"),
    );

    let mut db_client = db::connect(&config.rosslare_database).await;

    // Panic hook: dedicated connection so we don't need to fight for a lock.
    let panic_db = Arc::new(Mutex::new(Some(
        db::connect(&config.rosslare_database).await,
    )));

    // Revert any outstanding overrides from a previous crash.
    db::revert_all_overrides(&mut db_client, &local_db).await;

    // Resolve the panel address from the database.
    let network = db::resolve_target(&mut db_client, &config.proxy.target).await;
    let panel_addr = network.addr;

    // Override the panel address in the Rosslare database to point at us.
    db::apply_override(&mut db_client, &local_db, &network, config.proxy.listen).await;

    let db = Arc::new(AsyncMutex::new(db_client));

    println!();
    println!("AC-215 Proxy");
    println!("  Listen: {} (server connects here)", config.proxy.listen);
    println!("  Panel:  {panel_addr} (real panel)");
    println!("  API:    http://{}", config.api.listen);
    println!();

    // Construct handlers — keep typed references for the ones we need.
    let mut events_handler_inner = EventsHandler::new();
    {
        let ldb = local_db.clone();
        events_handler_inner.on_status(move |prev, curr| {
            api::webhooks::events::diff_outputs(&ldb, prev, curr);
        });
    }
    let events_handler = Arc::new(Mutex::new(events_handler_inner));
    let nack_handler = Arc::new(Mutex::new(NackHandler::new()));
    let logging_handler = Arc::new(Mutex::new(LoggingHandler));

    let handlers: Vec<Arc<Mutex<dyn FrameHandler>>> = vec![
        events_handler.clone(),
        nack_handler.clone(),
        logging_handler.clone(),
    ];

    let proxy = Proxy::new(
        config.proxy.listen,
        panel_addr,
        Cipher::new(),
        ChecksumMode::Auto,
        handlers,
    );

    let state = AppState {
        handle: proxy.handle(),
        nack_handler: nack_handler.clone(),
        events_handler: events_handler.clone(),
        local_db: local_db.clone(),
        db: db.clone(),
    };

    // Shutdown handler: revert override on Ctrl+C (direct mode).
    let shutdown_db = db.clone();
    let shutdown_local_db = local_db.clone();
    let shutdown_network_id = network.network_id;
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        info!("shutting down, reverting network override...");
        let mut client = shutdown_db.lock().await;
        db::revert_override(&mut client, &shutdown_local_db, shutdown_network_id).await;
        std::process::exit(0);
    });

    let panic_local_db = local_db.clone();
    let panic_network_id = network.network_id;
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        info!("panic detected, attempting to revert network override...");
        let Some(mut client) = panic_db.lock().unwrap().take() else {
            error!("panic db connection already consumed");
            default_hook(info);
            return;
        };
        let local_db = panic_local_db.clone();
        let handle = std::thread::spawn(move || {
            let Ok(rt) = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            else {
                error!("failed to build runtime for panic revert");
                return;
            };
            rt.block_on(db::revert_override(
                &mut client,
                &local_db,
                panic_network_id,
            ));
        });
        let _ = handle.join();
        default_hook(info);
    }));

    tokio::spawn(api::serve(config.api.listen, state));

    let listening = proxy.on_listening();
    tokio::spawn(async move { proxy.run().await });

    // Wait until the proxy is listening.
    listening.await.ok();

    // Report SERVICE_RUNNING if we're a service.
    if let Some((status_handle, shutdown_rx)) = service {
        use windows_service::service::*;

        let _ = status_handle.set_service_status(ServiceStatus {
            service_type: ServiceType::OWN_PROCESS,
            current_state: ServiceState::Running,
            controls_accepted: ServiceControlAccept::STOP,
            exit_code: ServiceExitCode::Win32(0),
            checkpoint: 0,
            wait_hint: std::time::Duration::ZERO,
            process_id: None,
        });

        // Wait for the SCM stop signal.
        shutdown_rx.await.ok();

        // Revert the override before stopping.
        info!("service stop requested, reverting network override...");
        let mut client = db.lock().await;
        db::revert_override(&mut client, &local_db, network.network_id).await;

        let _ = status_handle.set_service_status(ServiceStatus {
            service_type: ServiceType::OWN_PROCESS,
            current_state: ServiceState::Stopped,
            controls_accepted: ServiceControlAccept::empty(),
            exit_code: ServiceExitCode::Win32(0),
            checkpoint: 0,
            wait_hint: std::time::Duration::ZERO,
            process_id: None,
        });
    } else {
        // Direct mode — just wait forever.
        std::future::pending::<()>().await;
    }
}
