use lib8080_msg as lib;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::broadcast;
use tokio::sync::mpsc;
// is a sha256 hash
type ID = [u8; 32];
#[derive(Debug)]
pub struct State {
    pub user_book: HashMap<ID, lib::User>,
    // left is hash, right is actual name
    pub file_book: HashMap<String, String>,
    pub receiver: mpsc::Receiver<lib::Packet>,
    // a tokio broadcast internally clones the data, so we use Arc to prevent slowdowns from cloning
    pub msg_sender: broadcast::Sender<Arc<lib::Packet>>,
}
