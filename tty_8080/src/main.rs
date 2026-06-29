use clap::Parser;
use lib8080_msg::*;
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
    username: Option<Username>
}
fn main() {
    let args = Args::parse();
    
}
