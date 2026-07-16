mod prelude;
use crate::prelude::*;
#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Args {
    /// the ip adress of the server
    #[arg(short, long, value_name = "ADRESS")]
    adress: String,
    /// the port to connect to
    #[arg(short, long)]
    port: u16,
    #[arg(short, long)]
    /// the username to be used
    username: Username,
}
trait ErrorString: Into<StyledString> + Send + 'static {}
impl<T: Into<StyledString> + Send + 'static> ErrorString for T {}
type CursiveCallBack = Box<dyn FnOnce(&mut Cursive) + Send + 'static>;
/// create a new error which won't exit
fn non_fatal_error<S: ErrorString>(text: S) -> CursiveCallBack {
    Box::new(|siv| {
        siv.add_layer(
            Dialog::new()
                .content(TextView::new(text))
                .button("ok", |siv| {
                    siv.pop_layer();
                })
                .title("error")
                .h_align(HAlign::Center),
        );
    })
}
/// create a new error which will exit
fn fatal_error<S: ErrorString>(text: S) -> CursiveCallBack {
    Box::new(|siv| {
        siv.add_layer(
            Dialog::new()
                .content(TextView::new(text))
                .button("ok", |siv| {
                    siv.quit();
                })
                .title("error")
                .h_align(HAlign::Center),
        );
    })
}
/// decides if the message provided should or shouldn't be displayed
fn should_display_text(split_text: &[&str]) -> bool {
    let len = split_text.len();
    // we know that the message is never empty, so index zero should always be valid
    match split_text[0] {
        "/exit" | "/getport" => false,
        "/kick" | "file" if len == 2 => false,
        "/set_privilege" if len == 3 => false,
        _ => true,
    }
}
#[tokio::main]
async fn main() -> ExitCode {
    // error handeler
    match actual_main().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            ceprintln!("<bold><red>error:</> {e}");
            ExitCode::FAILURE
        }
    }
}
async fn actual_main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let (mut reader, mut writer) = TcpStream::connect(format!("{}:{}", args.adress, args.port))
        .await?
        .into_split();
    Packet::Join(args.username.clone())
        .send_async(&mut writer)
        .await?;
    let mut siv = Cursive::default();
    let cb_cink = siv.cb_sink().clone();
    let cb_clone = cb_cink.clone();
    let (tx, mut rx) = mpsc::unbounded_channel::<String>();
    // reads for messages from other users
    tokio::spawn(async move {
        let mut len_buff = [0u8; 4];
        loop {
            // only errors on early eof
            if reader.read_exact(&mut len_buff).await.is_err() {
                cb_cink
                    .send(fatal_error("error: lost connection with server"))
                    .expect("cursive cb channel should only close when program terminates");
                return;
            }
            let size_to_read = u32::from_be_bytes(len_buff) as usize;
            let mut raw_contents: Vec<u8> = vec![0; size_to_read];
            if reader.read_exact(&mut raw_contents).await.is_err() {
                cb_cink
                    .send(fatal_error("error: lost connection with server"))
                    .expect("cursive cb channel should only close when program terminates");
                return;
            }
            let contents: Packet = match serde_json::from_slice(&raw_contents) {
                Ok(c) => c,
                Err(e) => {
                    cb_cink
                        .send(non_fatal_error(format!(
                            "error: server send malformed data: {e}\nrecieved: {:?}",
                            raw_contents
                        )))
                        .expect("cursive cb channel should only close when program terminates");
                    continue;
                }
            };
            let Packet::Msg(m) = contents else {
                unreachable!("users can only send messages to eachother");
            };
            cb_cink
                .send(Box::new(move |siv| {
                    siv.call_on_name("text_buffer", |view: &mut TextView| {
                        view.append(format!("{m}\n"));
                    });
                }))
                .expect("cursive cb channel should only close when program terminates");
        }
    });
    let u_clone = args.username.clone();
    //sends messages to the server
    tokio::spawn(async move {
        let cb_cink = cb_clone;
        let username = u_clone;
        while let Some(msg) = rx.recv().await {
            let split_message: Vec<&str> = msg.split_whitespace().collect();
            let msg = match split_message[0] {
                "/exit" => Packet::Exit,
                "/getport" => Packet::GetPort,
                "/kick" if split_message.len() == 2 => Packet::Kick(split_message[1].to_string()),
                "/set_privilege" if split_message.len() == 3 => {
                    let privilege = match UserPrivilege::try_from(split_message[2]) {
                        Ok(p) => p,
                        Err(e) => {
                            let e = e.to_string();
                            cb_cink.send(non_fatal_error(format!("error: {e}"))).expect(
                                "cursive cb channel should only close when program terminates",
                            );
                            continue;
                        }
                    };
                    Packet::SetPrivilege(Some(split_message[1].to_string()), privilege)
                }
                _ => Packet::Msg(Message::new(&username, &msg)),
            };
            if msg.send_async(&mut writer).await.is_err() {
                cb_cink
                    .send(fatal_error("error: lost connection with server"))
                    .expect("cursive cb channel should only close when program terminates");
                return;
            }
            if let Packet::Exit = msg {
                cb_cink
                    .send(Box::new(|siv| siv.quit()))
                    .expect("cursive cb channel should only close when program terminates");
                return;
            }
        }
    });
    siv.add_fullscreen_layer(
        Dialog::new()
            .content(
                LinearLayout::vertical()
                    .child(
                        ScrollView::new(TextView::empty().with_name("text_buffer"))
                            .scroll_strategy(ScrollStrategy::StickToBottom)
                            .full_height(),
                    )
                    .child(
                        EditView::new()
                            .on_submit(move |siv, contents| {
                                if contents.trim().is_empty() {
                                    return;
                                }
                                let split_content: Vec<&str> =
                                    contents.split_whitespace().collect();
                                if should_display_text(&split_content) {
                                    siv.call_on_name("text_buffer", |view: &mut TextView| {
                                        view.append(format!("me: {contents}\n",));
                                    });
                                }
                                siv.call_on_name("input_field", |view: &mut EditView| {
                                    view.set_content("");
                                });
                                let _ = tx.send(contents.to_string());
                            })
                            .with_name("input_field"),
                    ),
            )
            .title("tty_8080")
            .full_height()
            .full_width(),
    );
    siv.run();
    Ok(())
}
