use tokio::{io::
    {self, AsyncReadExt, AsyncWriteExt}, 
    net::{TcpListener, tcp::OwnedWriteHalf}};
use tokio::sync::mpsc;
use tokio::sync::Mutex;
use libtty_msg::*;
use std::collections::HashMap;
use std::sync::Arc;
struct Client {
    user: User,
    stream: OwnedWriteHalf
}
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let user_book = Arc::new(Mutex::new(HashMap::new()));
    let listener = TcpListener::bind("0.0.0.0:0").await?;
    let (tx, mut rx) = mpsc::channel::<Packet>(100);
    let (shutdown_tx, mut shutdown_rx) = mpsc::channel::<()>(0);
    eprintln!("connect to port: {}", listener.local_addr()?.port());
    tokio::spawn(async move {
        let data = rx.recv().await.unwrap();
        eprintln!("{:?}", data);
    });
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
            tokio::spawn(async move {
                let (mut reader, writer) = connection.into_split(); 
                let mut size = [0u8; 4];
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
                if let Packet::Join(u) = data {
                    let username = u.get_username();
                    let client = Client {
                        user: u,
                        stream: writer
                    };
                    let mut user_book = user_book.lock().await;
                    if let Some(_) = user_book.get(&username) {
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
                    if let Err(_) = tx.send(serde_json::from_str(&data).unwrap()).await {
                        let _ = io::stderr().write("failed to send data\n".as_bytes()).await;
                    }
                }            
            });
        }
    });
    Ok(())
}
