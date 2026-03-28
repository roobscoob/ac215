use std::net::SocketAddr;

use log::{error, info, warn};
use tiberius::{AuthMethod, Client, Config, EncryptionLevel};
use tokio::net::windows::named_pipe::ClientOptions;
use tokio_util::compat::TokioAsyncReadCompatExt;

use crate::config::RosslareDatabaseConfig;
use crate::local_db::{LocalDb, NetworkOverride};

pub type DbClient = Client<tokio_util::compat::Compat<tokio::net::windows::named_pipe::NamedPipeClient>>;

pub async fn connect(config: &RosslareDatabaseConfig) -> DbClient {
    info!("connecting to SQL Server via named pipe: {}", config.pipe);

    let stream = ClientOptions::new()
        .open(&config.pipe)
        .expect("failed to open named pipe");

    let mut tib_config = Config::new();
    tib_config.host("localhost");
    tib_config.authentication(AuthMethod::sql_server(&config.username, &config.password));
    tib_config.database(&config.database);
    tib_config.trust_cert();
    tib_config.encryption(EncryptionLevel::NotSupported);

    Client::connect(tib_config, stream.compat())
        .await
        .expect("failed to connect to SQL Server")
}

/// A resolved network target: the panel address and its database row ID.
pub struct ResolvedNetwork {
    pub addr: SocketAddr,
    pub network_id: i64,
    pub ip: [u8; 4],
    pub port: i32,
}

/// Look up the panel address from tblNetworks by network description.
pub async fn resolve_target(client: &mut DbClient, target: &str) -> ResolvedNetwork {
    let row = client
        .query(
            "SELECT IdNetwork, wIp1, wIp2, wIp3, wIp4, iPort FROM tblNetworks WHERE tDescNetwork = @P1",
            &[&target],
        )
        .await
        .expect("failed to query tblNetworks")
        .into_row()
        .await
        .expect("failed to read row from tblNetworks")
        .unwrap_or_else(|| {
            panic!("no network found in tblNetworks with tDescNetwork = '{target}'")
        });

    let network_id: i32 = row.get("IdNetwork").expect("IdNetwork missing");
    let ip1: u8 = row.get("wIp1").expect("wIp1 missing");
    let ip2: u8 = row.get("wIp2").expect("wIp2 missing");
    let ip3: u8 = row.get("wIp3").expect("wIp3 missing");
    let ip4: u8 = row.get("wIp4").expect("wIp4 missing");
    let port: i32 = row.get("iPort").expect("iPort missing");
    assert!(
        port >= 0 && port <= 65535,
        "iPort {port} out of range for a TCP port"
    );

    let addr = SocketAddr::from(([ip1, ip2, ip3, ip4], port as u16));
    info!("resolved target '{target}' to {addr} (network_id={network_id})");

    ResolvedNetwork {
        addr,
        network_id: network_id as i64,
        ip: [ip1, ip2, ip3, ip4],
        port,
    }
}

/// Revert all outstanding overrides from a previous run.
pub async fn revert_all_overrides(client: &mut DbClient, local_db: &LocalDb) {
    let overrides = local_db.list_overrides().unwrap_or_else(|e| {
        error!("failed to list overrides: {e}");
        Vec::new()
    });

    for ov in &overrides {
        warn!(
            "reverting outstanding override for network_id={}: restoring {}.{}.{}.{}:{}",
            ov.network_id, ov.original_ip1, ov.original_ip2, ov.original_ip3, ov.original_ip4, ov.original_port
        );

        let result = client
            .execute(
                "UPDATE tblNetworks SET wIp1=@P1, wIp2=@P2, wIp3=@P3, wIp4=@P4, iPort=@P5 WHERE IdNetwork=@P6",
                &[
                    &(ov.original_ip1 as i16),
                    &(ov.original_ip2 as i16),
                    &(ov.original_ip3 as i16),
                    &(ov.original_ip4 as i16),
                    &ov.original_port,
                    &(ov.network_id as i32),
                ],
            )
            .await;

        match result {
            Ok(_) => {
                let _ = local_db.clear_override(ov.network_id);
                info!("reverted override for network_id={}", ov.network_id);
            }
            Err(e) => {
                error!(
                    "FAILED to revert override for network_id={}: {e}",
                    ov.network_id
                );
            }
        }
    }
}

/// Overwrite the network address in tblNetworks with the proxy's listen address,
/// saving the original in the local database.
pub async fn apply_override(
    client: &mut DbClient,
    local_db: &LocalDb,
    network: &ResolvedNetwork,
    proxy_listen: SocketAddr,
) {
    let proxy_ip: [u8; 4] = [127, 0, 0, 1];
    let proxy_port = proxy_listen.port() as i32;

    // Save the original before overwriting.
    let ov = NetworkOverride {
        network_id: network.network_id,
        original_ip1: network.ip[0],
        original_ip2: network.ip[1],
        original_ip3: network.ip[2],
        original_ip4: network.ip[3],
        original_port: network.port,
    };
    local_db
        .save_override(&ov)
        .expect("failed to save network override to local database");

    // Overwrite in Rosslare database.
    client
        .execute(
            "UPDATE tblNetworks SET wIp1=@P1, wIp2=@P2, wIp3=@P3, wIp4=@P4, iPort=@P5 WHERE IdNetwork=@P6",
            &[
                &(proxy_ip[0] as i16),
                &(proxy_ip[1] as i16),
                &(proxy_ip[2] as i16),
                &(proxy_ip[3] as i16),
                &proxy_port,
                &(network.network_id as i32),
            ],
        )
        .await
        .expect("failed to overwrite network address in tblNetworks");

    info!(
        "applied override for network_id={}: {}.{}.{}.{}:{} -> {proxy_listen}",
        network.network_id, network.ip[0], network.ip[1], network.ip[2], network.ip[3], network.port,
    );
}

/// Revert a single override. Used during shutdown.
pub async fn revert_override(client: &mut DbClient, local_db: &LocalDb, network_id: i64) {
    let Some(ov) = local_db.get_override(network_id).unwrap_or(None) else {
        return;
    };

    info!(
        "reverting override for network_id={}: restoring {}.{}.{}.{}:{}",
        ov.network_id, ov.original_ip1, ov.original_ip2, ov.original_ip3, ov.original_ip4, ov.original_port
    );

    let result = client
        .execute(
            "UPDATE tblNetworks SET wIp1=@P1, wIp2=@P2, wIp3=@P3, wIp4=@P4, iPort=@P5 WHERE IdNetwork=@P6",
            &[
                &(ov.original_ip1 as i16),
                &(ov.original_ip2 as i16),
                &(ov.original_ip3 as i16),
                &(ov.original_ip4 as i16),
                &ov.original_port,
                &(ov.network_id as i32),
            ],
        )
        .await;

    match result {
        Ok(_) => {
            let _ = local_db.clear_override(network_id);
            info!("reverted override for network_id={network_id}");
        }
        Err(e) => {
            error!("FAILED to revert override for network_id={network_id}: {e}");
        }
    }
}
