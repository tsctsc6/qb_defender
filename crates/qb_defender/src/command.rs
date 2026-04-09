use clap::Parser;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
pub struct Cli {
    #[arg(short, long)]
    pub port: u16,

    #[arg(short, long, default_value_t = 10)]
    pub interval: u64,

    /// Increase verbosity, repeat for more verbosity, default is 3 (info)
    #[arg(
        short = 'v',
        long,
        action = clap::ArgAction::Count,
        global = true,
        default_value_t = 3
    )]
    pub verbose: u8,
}
