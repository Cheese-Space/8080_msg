use tokio::io::AsyncReadExt;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpListener;
use tokio::sync::mpsc::{self, Sender};
use tokio::sync::Mutex;
use lib8080_msg::*;
use std::collections::HashMap;
use std::sync::Arc;
type UserBook = Arc<Mutex<HashMap<Username, Sender<Packet>>>>;
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let user_book: UserBook = Arc::new(Mutex::new(HashMap::new()));
    let listener = TcpListener::bind("0.0.0.0:0").await?;
    let port = listener.local_addr()?.port();
    let (shutdown_tx, mut shutdown_rx) = mpsc::channel(1);
    eprintln!("connect to port: {port}");
    // control-c handler
    tokio::spawn(async move {
        // bug: program could imedeatly stops if the ctrl_c function fails
        let _ = tokio::signal::ctrl_c().await;
        // it doesn't realy matter if the shutdown signal has been sent succsesfully, you can always force quit the server
        let _ = shutdown_tx.send(()).await;
    });
    // conection listening thread
    tokio::spawn(async move {
        loop {
            let (connection, _) = match listener.accept().await {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("failed to connect to user: {e}");
                    return;
                }
            };
            let user_book = Arc::clone(&user_book);
            // each connection gets its own thread could be prone to ddos atacks maybe?
            tokio::spawn(async move {
                let (mut reader, writer) = connection.into_split(); 
                let mut size = [0u8; 4];
                if let Err(_) = reader.read_exact(&mut size).await {
                    eprintln!("failed to register user");
                    return;
                }
                let size_to_read = u32::from_ne_bytes(size) as usize;
                let mut data = vec![0u8; size_to_read];
                if let Err(e) = reader.read_exact(&mut data).await {
                    eprintln!("failed to register user: {e}");
                    return;
                }
                let data: Packet = match serde_json::from_slice(&data) {
                    Ok(d) => d,
                    Err(e) => {
                        eprintln!("failed to register user: invalid request: {e}");
                        return;
                    }
                };
                // first packet should always be the join request, if not the user won't be registered
                let user = if let Packet::Join(u) = data {
                    let mut user = User::new(u, None);
                    let user_book = user_book.lock().await;
                    if user_book.is_empty() {
                        user.set_privelige(UserPrivelige::Admin);
                    }
                    let username = user.get_username();
                    if let Some(_) = user_book.get(username) {
                        eprintln!("failed to register user: an user with the same username alreaady exists");
                        return
                    }
                    user
                }
                else {
                    eprintln!("failed to register user: first request wasn't join request`");
                    return;
                };
                let (tx, mut rx) = mpsc::channel(50);
                let tx_clone = tx.clone();
                let mut user_book_guard = user_book.lock().await;
                user_book_guard.insert(user.get_username().to_string(), tx_clone);
                drop(user_book_guard);          
                // writer thread
                tokio::spawn(async move {
                    let user_book = Arc::clone(&user_book);
                    while let Some(packet) = rx.recv().await {
                        
                    }
                });
                // read loop
                loop {
                    if let Err(e) = reader.read_exact(&mut size).await {
                        eprintln!("failed to read packet: {e}");
                        continue;
                    }
                    let size = u32::from_ne_bytes(size);
                    let mut data = vec![0u8; size as usize];
                    if let Err(e) = reader.read_exact(&mut data).await {
                        eprintln!("failed to read packet: {e}");
                        continue;
                    }
                    let data: Packet = match serde_json::from_slice(&data) {
                        Ok(d) => d,
                        Err(e) => {
                            eprintln!("user send malformed packet: {e}");
                            continue;
                        }
                    };
                    // this means that the user has been kicked or the user exited
                    if let Err(_) = tx.send(data).await {
                        return;
                    }
                }            
            });
        }
    });
    // the main thread waits for shutdown
    shutdown_rx.recv().await;
    Ok(())
}
