#![deny(clippy::unwrap_used)]
#![deny(unused_mut)]
mod client_handler;
mod db;
mod macros;
mod prelude;
mod query;
mod static_messages;
use crate::db::DbExt;
use crate::prelude::*;
use crate::static_messages::*;
use sha2::Digest;
use sha2::Sha256;
use std::path::Path;
use tokio::sync::{broadcast, mpsc};
const N_MOST_RECENT: u32 = 30;
// sha256 hash
type ID = [u8; 32];
trait AbortExt {
    /// abort all currently running threads
    fn abort_all(&self);
}
impl AbortExt for [AbortHandle] {
    fn abort_all(&self) {
        for handle in self.iter() {
            handle.abort();
        }
    }
}
// TODO: add timeout to tls handshake
type ThreadBook = Arc<Mutex<Vec<AbortHandle>>>;
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut args = env::args();
    args.next();
    let cert_file = args
        .next()
        .ok_or_else(|| anyhow!("missing certificate file argument!"))?;
    let private_key_file = args
        .next()
        .ok_or_else(|| anyhow!("missing private key file argument!"))?;
    let config = Arc::new(
        ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(
                CertificateDer::pem_file_iter(cert_file)?.collect::<Result<_, _>>()?,
                PrivateKeyDer::from_pem_file(private_key_file)?,
            )?,
    );
    let acceptor = TlsAcceptor::from(config.clone());
    // init the logger
    pretty_env_logger::formatted_builder()
        .filter_level(LevelFilter::Info)
        .init();
    let listener = TcpListener::bind("0.0.0.0:0").await?;
    let port = listener.local_addr()?.port();
    let mut db_path = home_dir().unwrap_or_default();
    db_path.push("8080_msg.db");
    let db_client = PoolBuilder::new().path(&db_path).open().await?;
    db_client.conn(|conn| {
        conn.execute("CREATE TABLE IF NOT EXISTS Messages(id INTEGER PRIMARY KEY, user TEXT NOT NULL, message TEXT NOT NULL)", ())
    }).await?;
    let file_cache = TempDir::with_prefix(format!("8080_MSG_CACHE_{}", pid()))?;
    let cache_path: &'static Path = Box::leak(file_cache.path().to_path_buf().into_boxed_path());
    let thread_book: ThreadBook = Arc::new(Mutex::new(Vec::new()));
    let (msg_send, msg_recv) = broadcast::channel::<Arc<Packet>>(50);
    let (query_send, query_recv) = mpsc::channel::<query::Query>(50);
    let thread_clone = Arc::clone(&thread_book);
    let pool_clone = db_client.clone();
    #[cfg(debug_assertions)]
    info!("temp folder: {}", cache_path.display());
    info!("connect to port: {port}");
    // connection listening thread
    let listener_handle = tokio::spawn(async move {
        let thread_book = thread_clone;
        let db_client = pool_clone;
        loop {
            let (connection, _) = match listener.accept().await {
                Ok(c) => c,
                Err(e) => {
                    log_and!(return, "error", "failed to connect to user: {e}");
                }
            };
            let mut stream = match acceptor.accept(connection).await {
                Ok(s) => s,
                Err(e) => {
                    log_and!(return, "error", "failed to connect to user: {e}");
                }
            };
            let sender_clone = query_send.clone();
            let recv_clone = msg_recv.resubscribe();
            let db_clone = db_client.clone();
            // each connection gets its own thread could be prone to ddos attacks maybe?
            // TODO: maker user limit configurable
            let handle = tokio::spawn(async move {
                let db_client = db_clone;
                let query_sender = sender_clone;
                let mut msg_recv = recv_clone;
                // TODO: pull registering a user into its own function
                let size = match stream.read_u32().await {
                    Ok(s) => s as usize,
                    Err(e) => {
                        log_and!(return, "error", "failed to connect to user: {e}")
                    }
                };
                let mut data = vec![0u8; size];
                if let Err(e) = stream.read_exact(&mut data).await {
                    log_and!(return, "error", "failed to connect to user: {e}");
                }
                let Ok(Packet::Join(uname)) = serde_json::from_slice(&data) else {
                    log_and!(return, "error", "first request wasn't join request");
                };
                if let Err(e) = client_handler::username_check(&uname) {
                    log_and!(return, "error", "{e}");
                }
                let uid: ID = Sha256::digest(&uname).into();
                let (query, receiver) = query::Query::new(Packet::Join(uname), uid);
                // only way the recv is closed is when the query processing thread closes which only happens
                // when the program terminates, so we can just ignore the closed error
                let _ = query_sender.send(query).await;
                if let Err(e) = receiver
                    .await
                    .expect("other half shouldn't close before sending something")
                {
                    log_and!(return, "error", "{e}");
                }
                if let Err(e) = db_client
                    .send_n_most_recent(N_MOST_RECENT, &mut stream)
                    .await
                {
                    log::warn!("failed to send {N_MOST_RECENT} most recent messages: {e}");
                }
                loop {
                    let size = match stream.read_u32().await {
                        Ok(s) => s as usize,
                        Err(_) => {
                            client_handler::handle_closed_client(query_sender, uid).await;
                            return;
                        }
                    };
                    let mut data = vec![0u8; size];
                    if stream.read_exact(&mut data).await.is_err() {
                        client_handler::handle_closed_client(query_sender, uid).await;
                        return;
                    }
                    let data: Packet = match serde_json::from_slice(&data) {
                        Ok(p) => p,
                        Err(e) => log_and!(continue, "warn", "invalid packet sent: {e}"),
                    };
                    let (query, recv) = query::Query::new(data, uid);
                    let _ = query_sender.send(query).await;
                    let result = recv.await.expect("should not close before sending");
                    let message = match msg_recv.try_recv() {
                        Ok(m) => Vec::try_from(m.as_ref()).unwrap_or_default(),
                        Err(_) => Vec::new(),
                    };
                    let mut data = match result {
                        Ok(d) => d,
                        Err(e) => Vec::try_from(&Packet::Msg(e.into())).unwrap_or_default(),
                    };
                    data.reserve(message.len());
                    for byte in message {
                        data.push(byte);
                    }
                    let _ = stream.write_all(&data).await;
                    let _ = stream.flush().await;
                }
            });
            thread_book.lock().await.push(handle.abort_handle());
        }
    });
    // query handler
    let query_handle = tokio::spawn(async {
        todo!();
    });
    let mut thread_handle = thread_book.lock().await;
    thread_handle.push(query_handle.abort_handle());
    thread_handle.push(listener_handle.abort_handle());
    drop(thread_handle);
    // the main thread waits for control-c
    ctrl_c().await?;
    thread_book.lock().await.abort_all();
    Ok(())
}
