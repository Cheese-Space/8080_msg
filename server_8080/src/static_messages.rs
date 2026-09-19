use lib8080_msg::Message;
use lib8080_msg::Packet;
use std::sync::Arc;
use std::sync::LazyLock;
use std::sync::OnceLock;
// contains static messages used by server_8080 (along with the PORT static)
// all future static messages must be appended here
pub static PORT: OnceLock<u16> = OnceLock::new();
pub static GEPORT_MESSAGE: LazyLock<Packet> = LazyLock::new(|| {
    Packet::Msg(Message::new(
        "server",
        &format!(
            "port = {}",
            PORT.get()
                .expect("port should've been set before first used")
        ),
    ))
});
pub static KICK_MESSAGE: LazyLock<Arc<Packet>> =
    LazyLock::new(|| Arc::new(Packet::Msg(Message::new("server", "you have been kicked!"))));
pub static NO_KICK_PREMISION: LazyLock<Packet> = LazyLock::new(|| {
    Packet::Msg(Message::new(
        "server",
        "you don't have premission to kick people",
    ))
});
pub static NO_SEND_PREMISION: LazyLock<Arc<Packet>> = LazyLock::new(|| {
    Arc::new(Packet::Msg(Message::new(
        "server",
        "you don't have premission to send messages",
    )))
});
pub static NO_CHANGE_PRIVILEGE_PREMISION: LazyLock<Packet> = LazyLock::new(|| {
    Packet::Msg(Message::new(
        "server",
        "you don't have the premision to change the privilege of other users",
    ))
});
pub static CANT_CHANGE_OWN_PRIVILEGE: LazyLock<Arc<Packet>> = LazyLock::new(|| {
    Arc::new(Packet::Msg(Message::new(
        "server",
        "you can't change your own premission",
    )))
});
pub static CANT_KICK_YOURSELF: LazyLock<Packet> = LazyLock::new(|| {
    Packet::Msg(Message::new(
        "server",
        "you can't kick yourself\nhint if you want to exit, enter /exit or similar",
    ))
});
pub static INVALID_HASH: LazyLock<Packet> = LazyLock::new(|| {
    Packet::Msg(Message::new(
        "server",
        "failed to fetch file: file doesn't exist",
    ))
});
