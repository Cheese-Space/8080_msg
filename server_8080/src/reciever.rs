use tokio::sync::mpsc;
use lib8080_msg as lib;
use crate::{ID, query};
/// checks if a username is valid
// you normally shouldn't  return string literals as an Err, but this is only used in one place anyway so it is fine
pub fn username_check(username: &str) -> Result<(), &'static str> {
    match username.trim() {
        "" => Err("invalid username: an empty username isn't allowed"),
        "me" => Err("invalid username: me"),
        "server" => Err("invalid username: server"),
        with_space if with_space.contains(' ') => {
            Err("invalid username: a username isn't allowed to have spaces")
        }
        _ => Ok(()),
    }
}
// unlikely that the client would disconnect suddenly hence cold is used
#[cold]
pub async fn handle_closed_client(sender: mpsc::Sender<query::Query>, uid: ID) {
    log::error!("client closed");
    let (exit_query, _) = query::Query::new(lib::Packet::Exit, uid);
    let _ = sender.send(exit_query).await;
}