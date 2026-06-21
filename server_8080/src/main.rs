use tokio::{io::
    {self, AsyncReadExt, AsyncWriteExt}, net::{TcpListener, tcp::OwnedWriteHalf}, sync::oneshot::{self, Sender, error::TryRecvError}};
use tokio::sync::mpsc;
use tokio::sync::Mutex;
use lib8080_msg::*;
use std::collections::HashSet;
use std::sync::Arc;
struct Client {
    user: User,
    stream: OwnedWriteHalf,
    shutdown: Sender<()>
}
type UserBook = Arc<Mutex<HashSet<Username>>>;
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let user_book: UserBook = Arc::new(Mutex::new(HashSet::new()));
    let listener = TcpListener::bind("0.0.0.0:0").await?;
    let port = listener.local_addr()?.port();
    let (tx, mut rx) = mpsc::channel::<Packet>(100);
    let (shutdown_tx, mut shutdown_rx) = mpsc::channel(1);
    eprintln!("connect to port: {port}");
    let write_user_book = Arc::clone(&user_book);
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
            let tx = tx.clone();
            let user_book = Arc::clone(&user_book);
            // each connection gets its own thread could be prone to ddos atacks maybe?
            tokio::spawn(async move {
                let (mut reader, writer) = connection.into_split(); 
                let mut size = [0u8; 4];
                let (shutdown_tx, mut shutdown_rx) = oneshot::channel::<()>();
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
                let data: Packet = serde_json::from_slice(&data).unwrap();
                // first packet should always be the join request, if not the user won't be registered
                if let Packet::Join(u) = data {
                    let mut user = User::new(u, None);
                    let mut user_book = user_book.lock().await;
                    if user_book.is_empty() {
                        user.set_privelige(UserPrivelige::Admin);
                    }
                    let client = Client {
                        user: user,
                        stream: Arc::new(Mutex::new(writer)),
                        shutdown: shutdown_tx
                    };
                    let username = client.user.get_username();
                    if let Some(_) = user_book.get(username) {
                        eprintln!("failed to register user: an user with the same username alreaady exists");
                        return
                    }
                    user_book.insert(username.clone(), client);
                }
                else {
                    eprintln!("failed to register user: first request wasn't join request`");
                    return;
                }
                loop {
                    match shutdown_rx.try_recv() {
                        Ok(_) | Err(TryRecvError::Closed) => return,
                        Err(TryRecvError::Empty) => ()
                    }
                    if let Err(_) = reader.read_exact(&mut size).await {
                        eprintln!("failed to register user");
                        continue;
                    }
                    let size = u32::from_ne_bytes(size);
                    let mut data = vec![0u8; size as usize];
                    if let Err(_) = reader.read_exact(&mut data).await {
                        eprintln!("failed to register user");
                        continue;
                    }
                    let data = String::from_utf8(data).unwrap();
                    let data: Packet = serde_json::from_str(&data).unwrap();
                    if let Err(_) = tx.send(data).await {
                        eprintln!("failed to register user");
                    }
                }            
            });
        }
    });
    // the main thread waits for shutdown
    shutdown_rx.recv().await;
    Ok(())
}
