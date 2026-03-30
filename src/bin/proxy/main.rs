mod api;
mod config;
mod custom_event;
mod db;
pub mod event_emitter;
mod event_rewriter;
mod local_db;

use std::ffi::OsString;
use std::sync::{Arc, Mutex};

use clap::Parser;
use tokio::sync::Mutex as AsyncMutex;

use log::{error, info};

use ac215::crypto::Cipher;
use ac215::packet::header::ChecksumMode;
use ac215::proxy::Proxy;
use ac215::proxy::StatusTracker;
use ac215::proxy::handlers::nack::NackHandler;
use ac215::proxy::handlers::{EventsHandler, LoggingHandler, PanelHealthHandler};
use ac215::proxy::pipeline::FrameHandler;

use config::Config;
use local_db::LocalDb;

const SERVICE_NAME: &str = "Ac215Proxy";

#[derive(Parser)]
struct Args {
    /// Path to the configuration file
    #[arg(default_value = "proxy.toml")]
    config: String,

    /// Run as a Windows service (launched by the SCM)
    #[arg(long)]
    service: bool,
}

#[derive(Clone)]
pub struct AppState {
    pub handle: ac215::proxy::InterceptorHandle,
    pub nack_handler: Arc<Mutex<NackHandler>>,
    pub events_handler: Arc<Mutex<EventsHandler>>,
    pub local_db: Arc<LocalDb>,
    pub db: Arc<AsyncMutex<db::DbClient>>,
    pub status: StatusTracker,
}

fn main() {
    let args = Args::parse();

    if args.service {
        // Launched by the SCM — hand off to the service dispatcher.
        // Store the config path so service_main can retrieve it.
        CONFIG_PATH
            .set(args.config)
            .expect("config path already set");
        windows_service::service_dispatcher::start(SERVICE_NAME, ffi_service_main)
            .expect("failed to start service dispatcher");
    } else {
        // Running directly — use env_logger for console output.
        env_logger::init();
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("failed to build tokio runtime");
        rt.block_on(run(args.config, None));
    }
}

fn init_posthog_logger(api_key: &str) {
    use opentelemetry_otlp::{LogExporter, WithExportConfig, WithHttpConfig};

    let mut headers = std::collections::HashMap::new();
    headers.insert("Authorization".to_string(), format!("Bearer {api_key}"));

    let exporter = LogExporter::builder()
        .with_http()
        .with_endpoint("https://us.i.posthog.com/i/v1/logs")
        .with_headers(headers)
        .build()
        .expect("failed to build PostHog log exporter");

    let provider = opentelemetry_sdk::logs::SdkLoggerProvider::builder()
        .with_batch_exporter(exporter)
        .build();

    let bridge = opentelemetry_appender_log::OpenTelemetryLogBridge::new(&provider);
    log::set_boxed_logger(Box::new(bridge)).expect("failed to set logger");
    log::set_max_level(log::LevelFilter::Info);

    // Keep the provider alive for the lifetime of the process.
    LOGGER_PROVIDER
        .set(provider)
        .expect("logger provider already set");
}

static LOGGER_PROVIDER: std::sync::OnceLock<opentelemetry_sdk::logs::SdkLoggerProvider> =
    std::sync::OnceLock::new();

static CONFIG_PATH: std::sync::OnceLock<String> = std::sync::OnceLock::new();

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

    let config_path = CONFIG_PATH.get().expect("config path not set").clone();
    rt.block_on(async {
        run(config_path, Some((status_handle, shutdown_rx))).await;
    });
}

/// Core proxy logic. If `service` is provided, we report SERVICE_RUNNING
/// after the listener is up, and shut down on the stop signal.
async fn run(
    config_path: String,
    service: Option<(
        windows_service::service_control_handler::ServiceStatusHandle,
        tokio::sync::oneshot::Receiver<()>,
    )>,
) {
    let config = Config::load(&config_path);

    if service.is_some() {
        if let Some(ref posthog) = config.posthog {
            init_posthog_logger(&posthog.api_key);
        }
    }

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

    // Status tracker — shared across all components.
    let status = StatusTracker::new();

    // Construct handlers — keep typed references for the ones we need.
    let mut events_handler_inner = EventsHandler::new();
    events_handler_inner.set_status_tracker(status.clone());
    {
        let ldb = local_db.clone();
        events_handler_inner.on_status(move |prev, curr| {
            api::webhooks::events::diff_outputs(&ldb, prev, curr);
        });
    }
    {
        let ldb = local_db.clone();
        let db = db.clone();
        events_handler_inner.on_access(move |event| {
            api::webhooks::events::on_access(&ldb, &db, event);
        });
    }
    let events_handler = Arc::new(Mutex::new(events_handler_inner));
    let mut nack_handler_inner = NackHandler::new();
    nack_handler_inner.set_status_tracker(status.clone());
    let nack_handler = Arc::new(Mutex::new(nack_handler_inner));
    let logging_handler = Arc::new(Mutex::new(LoggingHandler));
    let panel_health_handler = Arc::new(Mutex::new(PanelHealthHandler::new(status.clone())));

    let handlers: Vec<Arc<Mutex<dyn FrameHandler>>> = vec![
        panel_health_handler.clone(),
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
        status.clone(),
    );

    let state = AppState {
        handle: proxy.handle(),
        nack_handler: nack_handler.clone(),
        events_handler: events_handler.clone(),
        local_db: local_db.clone(),
        db: db.clone(),
        status: status.clone(),
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
        let Ok(mut guard) = panic_db.try_lock() else {
            error!("panic db connection already locked");
            default_hook(info);
            return;
        };
        let Some(mut client) = guard.take() else {
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
