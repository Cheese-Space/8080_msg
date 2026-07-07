use tokio::io::AsyncReadExt;
use tokio::net::TcpListener;
use tokio::sync::mpsc::{self, Sender};
use tokio::sync::Mutex;
use lib8080_msg::*;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::LazyLock;
use std::sync::OnceLock;
#[macro_use]
extern crate log;
use log::LevelFilter;
static PORT: OnceLock<u16> = OnceLock::new();
static GEPORT_MESSAGE: LazyLock<Packet> = LazyLock::new(|| {
    Packet::Msg(Message::new("server", &format!("port = {}", PORT.get().unwrap())))
});
static KICK_MESSAGE: LazyLock<Packet> = LazyLock::new(|| {
    Packet::Msg(Message::new("server", "you have been kicked!"))
});
static NO_KICK_PREMISION: LazyLock<Packet> = LazyLock::new(|| {
   Packet::Msg(Message::new("server", "you don't have premision to kick people")) 
});
// it sends an Arc, so we don't have to clone when sending a Message to all writer thread
type UserBook = Arc<Mutex<HashMap<Username, Sender<Arc<Packet>>>>>;
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    pretty_env_logger::formatted_builder()
        .filter_level(LevelFilter::Info)
        .init();
    let user_book: UserBook = Arc::new(Mutex::new(HashMap::new()));
    let listener = TcpListener::bind("0.0.0.0:0").await?;
    let port = listener.local_addr()?.port();
    PORT.set(port).expect("PORT should be unset");
    let (shutdown_tx, mut shutdown_rx) = mpsc::channel(1);
    info!("connect to port: {port}");
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
                    error!("failed to connect to user: {e}");
                    return;
                }
            };
            let user_book = Arc::clone(&user_book);
            // each connection gets its own thread could be prone to ddos atacks maybe?
            tokio::spawn(async move {
                let (mut reader, mut writer) = connection.into_split(); 
                let mut size = [0u8; 4];
                if let Err(_) = reader.read_exact(&mut size).await {
                    error!("failed to register user");
                    return;
                }
                let size_to_read = u32::from_be_bytes(size) as usize;
                let mut data = vec![0u8; size_to_read];
                if let Err(e) = reader.read_exact(&mut data).await {
                    error!("failed to register user: {e}");
                    return;
                }
                let data: Packet = match serde_json::from_slice(&data) {
                    Ok(d) => d,
                    Err(e) => {
                        error!("failed to register user: invalid request: {e}");
                        return;
                    }
                };
                // first packet should always be the join request, if not the user won't be registered
                let user = if let Packet::Join(u) = data {
                    let mut user = User::new(&u, None);
                    let user_book = user_book.lock().await;
                    if user_book.is_empty() {
                        user.set_privilege(UserPrivilege::Admin);
                    }
                    let username = user.get_username();
                    if let Some(_) = user_book.get(username) {
                        error!("failed to register user: an user with the same username alreaady exists");
                        return
                    }
                    user
                }
                else {
                    error!("failed to register user: first request wasn't join request`");
                    return;
                };
                let (tx, mut rx) = mpsc::channel(50);
                let tx_clone = tx.clone();
                let mut user_book_guard = user_book.lock().await;
                user_book_guard.insert(user.get_username().to_string(), tx_clone);
                info!("user '{}' joined", user.get_username());
                drop(user_book_guard); 
                let u_book_clone = Arc::clone(&user_book);
                // writer thread
                let u_clone = user.clone();
                tokio::spawn(async move {
                    let user_book = u_book_clone;
                    let user = u_clone;
                    while let Some(packet) = rx.recv().await {
                        info!("recieved packet: {:?}", *packet);
                        match packet.as_ref() {
                            Packet::Exit => {
                                // the read thread will auto remove on failiure anyway so we only have to break the write loop
                                break;
                            }
                            Packet::Join(_) => (), // joining is handled in the reader thread
                            Packet::GetPort => {
                                let _ = GEPORT_MESSAGE.send_async(&mut writer).await;
                            }
                            Packet::Kick(u) => {
                                if !matches!(user.get_privilege(), UserPrivilege::Admin) {
                                    let _ = NO_KICK_PREMISION.send_async(&mut writer).await;
                                    continue;
                                }
                                let mut user_book = user_book.lock().await;
                                let sender = match user_book.remove(u) {
                                    Some(s) => s,
                                    None => {
                                        drop(user_book);
                                        let msg = Packet::Msg(Message::new("server", &format!("{u} doesn't exist")));
                                        let _ = msg.send_async(&mut writer).await;
                                        continue;
                                    }
                                };
                                drop(user_book);
                                // kinda defeats the purpose of LazyLock but thats not important for now
                                let _ = sender.send(Arc::new(KICK_MESSAGE.clone())).await;
                                // exit makes the write loop stop
                                let _ = sender.send(Arc::new(Packet::Exit)).await;
                            }
                            // we know it is a Packet::Msg, if we matched the normal way, we would need to clone the inner message to avoid moving it
                            msg => {
                                if msg.get_inner_msg().expect("should be a Msg").get_username() == user.get_username() {
                                    // the client is responsible for displaying its own messages
                                    continue;
                                }
                                let _ = msg.send_async(&mut writer).await;
                            }
                        }
                    }
                });
                // read loop
                loop {
                    // this means the user disconnected suddenly
                    if let Err(e) = reader.read_exact(&mut size).await {
                        error!("failed to read packet: {e}\n removing: {}", user.get_username());
                        let mut user_book = user_book.lock().await;
                        user_book.remove(user.get_username());
                        return;
                    }
                    let size = u32::from_be_bytes(size);
                    let mut data = vec![0u8; size as usize];
                    if let Err(e) = reader.read_exact(&mut data).await {
                        error!("lost connection to cliet: {e}\nwill remove: {}", user.get_username());
                        let mut user_book = user_book.lock().await;
                        user_book.remove(user.get_username());
                        return;
                    }
                    let data: Packet = match serde_json::from_slice(&data) {
                        Ok(d) => d,
                        Err(e) => {
                            error!("user send malformed packet: {e}");
                            continue;
                        }
                    };
                    if let Packet::Msg(m) = data {
                        let user_book = user_book.lock().await;
                        let mut senders = Vec::with_capacity(user_book.len());
                        for (_, sender) in user_book.iter() {
                            senders.push(sender.clone());
                        }
                        let packet = Arc::new(Packet::Msg(m));
                        drop(user_book);
                        for sender in senders {
                            let packet_clone = Arc::clone(&packet);
                            let _ = sender.send(packet_clone).await;
                        }
                    }
                    else {
                        // this means that the user has been kicked or the user exited
                        if let Err(_) = tx.send(Arc::new(data)).await {
                            return;
                        }
                    }
                    
                }            
            });
        }
    });
    // the main thread waits for shutdown
    shutdown_rx.recv().await;
    Ok(())
}