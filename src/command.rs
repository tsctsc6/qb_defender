use clap::Parser;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
pub struct Cli {
    #[arg(short, long)]
    pub port: u16,

    #[arg(short, long, default_value_t = 10)]
    pub interval: u64,
}