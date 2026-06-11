use tokio::{io::
    {self, AsyncReadExt, AsyncWriteExt}, net::{TcpListener, tcp::OwnedWriteHalf}, sync::oneshot::{self, Sender, error::TryRecvError}, task::JoinHandle};
use tokio::sync::mpsc;
use tokio::sync::Mutex;
use libtty_msg::*;
use std::collections::HashMap;
use std::sync::Arc;
struct Client {
    user: User,
    stream: OwnedWriteHalf,
    shutdown: Sender<()>
}
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let user_book = Arc::new(Mutex::new(HashMap::<Username, Client>::new()));
    let listener = TcpListener::bind("0.0.0.0:0").await?;
    let port = listener.local_addr()?.port();
    let (tx, mut rx) = mpsc::channel::<Packet>(100);
    let (shutdown_tx, mut shutdown_rx) = mpsc::channel(1);
    eprintln!("connect to port: {port}");
    let write_user_book = Arc::clone(&user_book);
    // writer thread
    tokio::spawn(async move {
        let user_book = write_user_book;
        while let Some(packet) = rx.recv().await {
            match packet {
                Packet::Exit(u) => {
                    let mut user_book = user_book.lock().await;
                    let username = u.get_username();
                    let client = user_book.remove(&username).unwrap();
                    let _ = client.shutdown.send(());                
                    drop(user_book);
                    let _ = io::stderr().write(format!("user {username} left\n").as_bytes());
                }
                Packet::Join(_) => unreachable!("joining is handeled in de read thread, because we have to know if it is the first request"),
                Packet::Kick(u) => {
                    if !matches!(u.get_privelige(), UserPrivelige::Admin) {
                        
                    }
                }
                _ => todo!()
            }
        }
        // if there are no more users connected, the server shuts down
        let _ = shutdown_tx.send(());
    });
    // conection listening thread
    tokio::spawn(async move {
        loop {
            let (connection, _) = match listener.accept().await {
                Ok(c) => c,
                Err(_) => {
                    let _ = io::stderr().write("failed to connect to user\n".as_bytes()).await;
                    return;
                }
            };
            let tx = tx.clone();
            let user_book = Arc::clone(&user_book);
            // each connection gets its own thread
            tokio::spawn(async move {
                let (mut reader, writer) = connection.into_split(); 
                let mut size = [0u8; 4];
                let (shutdown_tx, mut shutdown_rx) = oneshot::channel::<()>();
                if let Err(_) = reader.read_exact(&mut size).await {
                    let _ = io::stderr().write("failed to register user\n".as_bytes()).await;
                    return;
                }
                let size_to_read = u32::from_ne_bytes(size) as usize;
                let mut data = vec![0u8; size_to_read];
                if let Err(_) = reader.read_exact(&mut data).await {
                    let _ = io::stderr().write("failed to register user\n".as_bytes()).await;
                    return;
                }
                let data: Packet = serde_json::from_slice(&data).unwrap();
                // first packet should always be the join request, if not the user won't be registered
                if let Packet::Join(mut u) = data {
                    let username = u.get_username();
                    let mut user_book = user_book.lock().await;
                    if user_book.is_empty() {
                        u.set_privelige(UserPrivelige::Admin);
                    }
                    let client = Client {
                        user: u,
                        stream: writer,
                        shutdown: shutdown_tx
                    };
                    if let Some(_) = user_book.get(&username) {
                        drop(user_book);
                        let _ = io::stderr().write("failed to register user\nsame username\n".as_bytes()).await;
                        return
                    }
                    user_book.insert(username, client);
                }
                else {
                    let _ = io::stderr().write("failed to register user\n".as_bytes()).await;
                    return;
                }
                loop {
                    match shutdown_rx.try_recv() {
                        Ok(_) | Err(TryRecvError::Closed) => return,
                        Err(TryRecvError::Empty) => ()
                    }
                    if let Err(_) = reader.read_exact(&mut size).await {
                        let _ = io::stderr().write("failed to read data\n".as_bytes()).await;
                        continue;
                    }
                    let size = u32::from_ne_bytes(size);
                    let mut data = vec![0u8; size as usize];
                    if let Err(_) = reader.read_exact(&mut data).await {
                        let _ = io::stderr().write("failed to read data\n".as_bytes()).await;
                        continue;
                    }
                    let data = String::from_utf8(data).unwrap();
                    let data: Packet = serde_json::from_str(&data).unwrap();
                    if let Err(_) = tx.send(data).await {
                        let _ = io::stderr().write("failed to send data\n".as_bytes()).await;
                    }
                }            
            });
        }
    });
    // the main thread waits for shutdown
    shutdown_rx.recv().await;
    Ok(())
}
