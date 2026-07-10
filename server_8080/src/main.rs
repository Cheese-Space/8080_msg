use async_sqlite::PoolBuilder;
use tokio::io::AsyncReadExt;
use tokio::net::TcpListener;
use tokio::sync::mpsc::{self, UnboundedSender};
use tokio::sync::Mutex;
use tokio::sync::RwLock;
use tokio::signal::ctrl_c;
use lib8080_msg::*;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::LazyLock;
use std::sync::OnceLock;
use std::env::home_dir;
#[macro_use]
extern crate log;
use log::LevelFilter;
static PORT: OnceLock<u16> = OnceLock::new();
// static messages send by the server
static GEPORT_MESSAGE: LazyLock<Packet> = LazyLock::new(|| {
    Packet::Msg(Message::new("server", &format!("port = {}", PORT.get().expect("port should've been set before first used"))))
});
static KICK_MESSAGE: LazyLock<Arc<Packet>> = LazyLock::new(|| {
    Arc::new(Packet::Msg(Message::new("server", "you have been kicked!")))
});
static NO_KICK_PREMISION: LazyLock<Packet> = LazyLock::new(|| {
   Packet::Msg(Message::new("server", "you don't have premision to kick people")) 
});
static NO_CHANGE_PRIVILEGE_PREMISION: LazyLock<Packet> = LazyLock::new(|| {
    Packet::Msg(Message::new("server", "you don't have the premision to change the privilege of other users"))
});
static CANT_CHANGE_OWN_PRIVILEGE: LazyLock<Arc<Packet>> = LazyLock::new(|| {
    Arc::new(Packet::Msg(Message::new("server", "you can't change your own premision")))
});
// it sends an Arc, so we don't have to clone when sending a Message to all writer thread
type UserBook = Arc<Mutex<HashMap<Username, UnboundedSender<Arc<Packet>>>>>;
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // init the logger
    pretty_env_logger::formatted_builder()
        .filter_level(LevelFilter::Info)
        .init();
    // user_book keeps track of users
    let user_book: UserBook = Arc::new(Mutex::new(HashMap::new()));
    let listener = TcpListener::bind("0.0.0.0:0").await?;
    let port = listener.local_addr()?.port();
    PORT.set(port).expect("PORT should be unset");
    let mut db_path = home_dir().unwrap_or_default();
    db_path.push("8080_msg.db");
    let db_client = PoolBuilder::new()
        .path(format!("{}", db_path.display()))
        .open().await?;
    db_client.conn(|conn| {
        conn.execute("CREATE TABLE IF NOT EXISTS Messages(id INTEGER PRIMARY KEY, user TEXT NOT NULL, message TEXT NOT NULL)", ())
    }).await?;
    info!("connect to port: {port}");
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
            let db_client = db_client.clone();
            // each connection gets its own thread could be prone to ddos atacks maybe?
            // todo: maker user limit configurable
            tokio::spawn(async move {
                let (mut reader, mut writer) = connection.into_split(); 
                let mut size = [0u8; 4];
                if reader.read_exact(&mut size).await.is_err() {
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
                    if user_book.get(username).is_some() {
                        error!("failed to register user: an user with the same username alreaady exists");
                        return
                    }
                    user
                }
                else {
                    error!("failed to register user: first request wasn't join request`");
                    return;
                };
                let user = Arc::new(RwLock::new(user));
                let (tx, mut rx) = mpsc::unbounded_channel();
                let tx_clone = tx.clone();
                let mut user_book_guard = user_book.lock().await;
                let user_read_guard = user.read().await;
                user_book_guard.insert(user_read_guard.get_username().to_string(), tx_clone);
                info!("user '{}' joined", user_read_guard.get_username());
                drop(user_book_guard); 
                drop(user_read_guard);
                let u_book_clone = Arc::clone(&user_book);
                // writer thread
                let u_clone = Arc::clone(&user);
                let u_clone_2 = Arc::clone(&user);
                let tx_clone_2 = tx.clone();
                // send at most 30 most recent messages to the client
                if let Err(e) = db_client.conn(move |conn| {
                    let user = u_clone_2;
                    let tx = tx_clone_2;
                    let mut stmt = conn.prepare("SELECT user, message FROM Messages WHERE id > (SELECT MAX(id) - 30 FROM Messages)")?;
                    let packets = stmt.query_map((), |row| {
                        let mut username: String = row.get("user")?;
                        if username.as_str() == user.blocking_read().get_username() {
                            username = String::from("me");
                        } 
                        let message: String = row.get("message")?;
                        Ok(Packet::Msg(Message::new(&username, &message)))
                    })?;
                    for packet in packets {
                        let packet = packet?;
                        let _ = tx.send(Arc::new(packet));
                    }
                    Ok(())
                }).await {
                    error!("failed to query database: {e}");
                }
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
                                if !matches!(user.read().await.get_privilege(), UserPrivilege::Admin) {
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
                                let _ = sender.send(Arc::clone(&KICK_MESSAGE));
                                // exit makes the write loop stop
                                let _ = sender.send(Arc::new(Packet::Exit));
                            }
                            Packet::SetPrivilege(u, p) => {
                                if u.is_none() {
                                    user.write().await.set_privilege(*p);
                                    continue;
                                }
                                if !matches!(user.read().await.get_privilege(), UserPrivilege::Admin) {
                                    let _ = NO_CHANGE_PRIVILEGE_PREMISION.send_async(&mut writer).await;
                                    continue;
                                }
                                let user_book = user_book.lock().await;
                                let username = u.as_ref().expect("we checked if u is None");
                                if username == user.read().await.get_username() {
                                    // a user shouldn't be able to set his own privilege
                                    let _ = CANT_CHANGE_OWN_PRIVILEGE.send_async(&mut writer).await;
                                    continue;
                                }
                                let sender = match user_book.get(username) {
                                    Some(s) => s,
                                    None => {
                                        drop(user_book);
                                        let msg = Packet::Msg(Message::new("server", &format!("{username} doesn't exist")));
                                        let _ = msg.send_async(&mut writer).await;
                                        continue;
                                    }
                                };
                                let _ = sender.send(Arc::new(Packet::SetPrivilege(None, *p)));
                            }
                            // we know it is a Packet::Msg, if we matched the normal way, we would need to clone the inner message to avoid moving it
                            msg => {
                                if msg.get_inner_msg().expect("should be a Msg").get_username() == user.read().await.get_username() {
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
                        let user = user.read().await;
                        let username = user.get_username();
                        error!("failed to read packet: {e}\n removing: {username}");
                        let mut user_book = user_book.lock().await;
                        user_book.remove(username);
                        return;
                    }
                    let size = u32::from_be_bytes(size);
                    let mut data = vec![0u8; size as usize];
                    if let Err(e) = reader.read_exact(&mut data).await {
                        let user = user.read().await;
                        let username = user.get_username();
                        error!("lost connection to cliet: {e}\nwill remove: {username}");
                        let mut user_book = user_book.lock().await;
                        user_book.remove(username);
                    }
                    let data: Packet = match serde_json::from_slice(&data) {
                        Ok(d) => d,
                        Err(e) => {
                            error!("user send malformed packet: {e}");
                            continue;
                        }
                    };
                    if let Packet::Msg(m) = data {
                        // add it to the database
                        // we don't add it in the writer thread, because then server messages would be inserted into the db
                        let user = m.get_username().to_string();
                        let message = m.get_message().to_string();
                        if let Err(e) = db_client.conn(|conn| {
                            conn.execute("INSERT INTO Messages (user, message) VALUES (?, ?)", [user, message])
                        }).await {
                            error!("failed to insert message into db: {e}");
                        }
                        let user_book = user_book.lock().await;
                        let mut senders = Vec::with_capacity(user_book.len());
                        for sender in user_book.values() {
                            senders.push(sender.clone());
                        }
                        let packet = Arc::new(Packet::Msg(m));
                        drop(user_book);
                        for sender in senders {
                            let packet_clone = Arc::clone(&packet);
                            let _ = sender.send(packet_clone);
                        }
                    }
                    else if let Packet::SetPrivilege(None, _) = data {
                        // a user should'nt be able to set his own privilege
                        let _ = tx.send(Arc::clone(&CANT_CHANGE_OWN_PRIVILEGE));
                        continue;
                    }
                    else {
                        // this means that the user has been kicked or the user exited
                        if tx.send(Arc::new(data)).is_err() {
                            return;
                        }
                    }
                    
                }            
            });
        }
    });
    // the main thread waits for control-c
    ctrl_c().await?;
    Ok(())
}