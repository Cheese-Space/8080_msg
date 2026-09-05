// use expect when you know the result to always be Ok (PROVIDE A REASON)
#![deny(clippy::unwrap_used)]
mod prelude;
use crate::prelude::*;
const MAIN_STACK: &str = "__MAIN_STACK__";
macro_rules! version {
    () => {
        "0.2.0 BETA"
    };
}
// avoids having to use format! for unexpected runtime errors
macro_rules! debug_msg {
    (name not found) => {
        concat!("internal error: name not found\nNote you SHOULDN'T see this message in a release build of tty_8080.\nIf you are seeing this in a release build, please make a new issue on github with the folowing information:\nissue type: name not found in stackview\nversion: ", version!())
    };
}
trait CursiveStackExt {
    fn push_layer_stack<T: View>(&mut self, view: T);
    fn push_fullscreen<T: View>(&mut self, view: T);
    fn pop_layer_stack(&mut self);
    fn move_layer_to_front(&mut self, name: &str);
    fn move_layer_to_back(&mut self, name: &str);
}
impl CursiveStackExt for Cursive {
    fn push_layer_stack<T: View>(&mut self, view: T) {
        self.call_on_name(MAIN_STACK, |stack: &mut StackView| {
            stack.add_layer(view);
        });
    }
    fn push_fullscreen<T: View>(&mut self, view: T) {
        self.call_on_name(MAIN_STACK, |stack: &mut StackView| {
            stack.add_fullscreen_layer(view);
        });
    }
    fn pop_layer_stack(&mut self) {
        self.call_on_name(MAIN_STACK, |stack: &mut StackView| {
            stack.pop_layer();
        });
    }
    fn move_layer_to_front(&mut self, name: &str) {
        self.call_on_name(MAIN_STACK, |stack: &mut StackView| {
            let layer = stack
                .find_layer_from_name(name)
                .expect(debug_msg!(name not found));
            stack.move_to_front(layer);
        });
    }
    fn move_layer_to_back(&mut self, name: &str) {
        self.call_on_name(MAIN_STACK, |stack: &mut StackView| {
            let layer = stack
                .find_layer_from_name(name)
                .expect(debug_msg!(name not found));
            stack.move_to_back(layer);
        });
    }
}
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
/// create a new error which won't exit
fn non_fatal_error<S: ErrorString>(text: S) -> impl FnOnce(&mut Cursive) {
    |siv| {
        siv.add_layer(
            Dialog::new()
                .content(TextView::new(text))
                .button("ok", |siv| {
                    siv.pop_layer();
                })
                .title("error")
                .h_align(HAlign::Center),
        );
    }
}
/// create a new error which will exit
fn fatal_error<S: ErrorString>(text: S) -> impl FnOnce(&mut Cursive) {
    |siv| {
        siv.add_layer(
            Dialog::new()
                .content(TextView::new(text))
                .button("ok", |siv| {
                    siv.quit();
                })
                .title("error")
                .h_align(HAlign::Center),
        );
    }
}
/// decides if the message provided should or shouldn't be displayed
fn should_display_text(split_text: &[&str]) -> bool {
    let len = split_text.len();
    // we know that the message is never empty, so index zero should always be valid
    match split_text[0] {
        "/exit" | "/getport" | "/file" => false,
        "/kick" if len == 2 => false,
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
    let mut siv = Cursive::default();
    let args = Args::parse();
    let (mut reader, mut writer) = TcpStream::connect(format!("{}:{}", args.adress, args.port))
        .await?
        .into_split();
    Packet::Join(args.username.clone())
        .send_async(&mut writer)
        .await?;
    siv.add_fullscreen_layer(StackView::new().with_name(MAIN_STACK));
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
                    .send(Box::new(fatal_error("error: lost connection with server")))
                    .expect("cursive cb channel should only close when program terminates");
                return;
            }
            let size_to_read = u32::from_be_bytes(len_buff) as usize;
            let mut raw_contents: Vec<u8> = vec![0; size_to_read];
            if reader.read_exact(&mut raw_contents).await.is_err() {
                cb_cink
                    .send(Box::new(fatal_error("error: lost connection with server")))
                    .expect("cursive cb channel should only close when program terminates");
                return;
            }
            let content: Packet = match serde_json::from_slice(&raw_contents) {
                Ok(c) => c,
                Err(e) => {
                    cb_cink
                        .send(Box::new(non_fatal_error(format!(
                            "error: server send malformed data: {e}\nrecieved: {:?}",
                            raw_contents
                        ))))
                        .expect("cursive cb channel should only close when program terminates");
                    continue;
                }
            };
            let m = match content {
                Packet::Msg(m) => m,
                Packet::File(_) => todo!("redo file handeling (first attempt didn't work)"),
                _ =>
                /* SAFETY: code will never be reached as users can only recieve files and messages from other users */
                unsafe { unreachable_unchecked() },
            };
            cb_cink
                .send(Box::new(move |siv| {
                    siv.call_on_name("text_buffer", |view: &mut TextView| {
                        view.append(format!("{}: {m}\n", m.get_username()));
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
        let writer = Arc::new(Mutex::new(writer));
        while let Some(msg) = rx.recv().await {
            let split_message: Vec<&str> = msg.split_whitespace().collect();
            let len = split_message.len();
            let msg = match split_message[0] {
                "/exit" => Packet::Exit,
                "/getport" => Packet::GetPort,
                "/kick" if len == 2 => Packet::Kick(split_message[1].to_string()),
                "/set_privilege" if len == 3 => {
                    let privilege = match UserPrivilege::try_from(split_message[2]) {
                        Ok(p) => p,
                        Err(e) => {
                            let e = e.to_string();
                            cb_cink
                                .send(Box::new(non_fatal_error(format!("error: {e}"))))
                                .expect(
                                    "cursive cb channel should only close when program terminates",
                                );
                            continue;
                        }
                    };
                    Packet::SetPrivilege(Some(split_message[1].to_string()), privilege)
                }
                "/file" => {
                    // file transfers are handled in a different thread so we can still send messages if reading takes a long itme
                    let writer = Arc::clone(&writer);
                    let cb_cink = cb_cink.clone();
                    let username = username.clone();
                    tokio::spawn(async move {
                        let (sender, reciever) = oneshot::channel::<(String, String)>();
                        let sender = StdMutex::new(Some(sender));
                        let cb_clone = cb_cink.clone();
                        cb_cink.send(Box::new(move |siv| {
                            siv.add_layer(Dialog::new().content(
                                LinearLayout::vertical()
                                    .child(TextView::new("enter path:"))
                                    .child(EditView::new().with_name("path"))
                                    .child(TextView::new("enter message:"))
                                    .child(EditView::new().with_name("message"))
                            ).button("send", move |siv| {
                                let mut path = None;
                                siv.call_on_name("path", |view: &mut EditView| {
                                    let input = view.get_content();
                                    if input.trim().is_empty() {
                                        let error = non_fatal_error("error: no path provided");
                                        cb_clone.send(Box::new(error)).expect("cursive cb channel should only close when program terminates");
                                        return;
                                    }
                                    path = Some(input.to_string());
                                });
                                let mut message = None;
                                siv.call_on_name("message", |view: &mut EditView| {
                                    let input = view.get_content();
                                    if input.trim().is_empty() {
                                        let error = non_fatal_error("error: no message provided");
                                        cb_clone.send(Box::new(error)).expect("cursive cb channel should only close when program terminates");
                                        return;
                                    }
                                    message = Some(input.to_string());
                                });
                                let path = match path {
                                    Some(p) => p,
                                    None => return,
                                };
                                let message = match message {
                                    Some(m) => m,
                                    None => return,
                                };
                                let Some(sender) = sender.lock().unwrap().take() else {
                                    unreachable!("should only be called once");
                                };
                                sender.send((path, message)).expect("reciever shouldn't have closed yet");
                                siv.pop_layer();
                            })
                            .button("cancel", |siv| {siv.pop_layer();})
                            .h_align(HAlign::Center));
                        })).expect("cursive cb channel should only close when program terminates");
                        let (path, msg) = match reciever.await {
                            Ok(pm) => pm,
                            Err(_) => return, // this means cancel was pressed
                        };
                        let file = match UserFile::new_async(&path).await {
                            Ok(f) => f,
                            Err(e) => {
                                cb_cink.send(Box::new(non_fatal_error(format!("error: failed to read file: {e}")))).expect("cursive cb channel should only close when program terminates");
                                return;
                            }
                        };
                        let msg = Message::new(&username, &msg);
                        let packet = Packet::File(FileTransfer::new(msg, file));
                        if packet.send_async(&mut *writer.lock().await).await.is_err() {
                            cb_cink
                                .send(Box::new(fatal_error("error: lost connection with server")))
                                .expect(
                                    "cursive cb channel should only close when program terminates",
                                );
                        }
                    });
                    continue;
                }
                _ => Packet::Msg(Message::new(&username, &msg)),
            };
            if msg.send_async(&mut *writer.lock().await).await.is_err() {
                cb_cink
                    .send(Box::new(fatal_error("error: lost connection with server")))
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
    siv.push_fullscreen(
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
            .full_width()
            .with_name("main"),
    );
    siv.run();
    Ok(())
}
