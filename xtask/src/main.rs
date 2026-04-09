use std::{fs::File, path::Path, sync::OnceLock};

use anyhow::{Context, Ok, Result};
use cargo_metadata::MetadataCommand;
use clap::{Parser, Subcommand, ValueEnum};
use walkdir::WalkDir;
use xshell::{Shell, cmd};
use zip::{ZipWriter, write::SimpleFileOptions};

#[derive(Parser)]
#[command(name = "xtask")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Build and move to dist folder
    Publish,
    /// Get metadata
    Meta {
        #[arg(short, long, value_enum, default_value_t = MetaField::Name)]
        field: MetaField,
    },
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
enum MetaField {
    Name,
    Version,
    HostName,
    AssetsName,
}

static NAME: OnceLock<String> = OnceLock::new();
static VERSION: OnceLock<String> = OnceLock::new();
static HOST_NAME: OnceLock<String> = OnceLock::new();

fn main() -> Result<()> {
    let cli = Cli::parse();

    get_metadata()?;

    match cli.command {
        Commands::Publish => {
            release()?;
            package()?;
            println!("Done.");
        }
        Commands::Meta { field } => match field {
            MetaField::Name => println!("{}", get_name()),
            MetaField::Version => println!("{}", get_version()),
            MetaField::HostName => println!("{}", get_host_name()),
            MetaField::AssetsName => println!("{}", get_target_folder_name()),
        },
    }

    Ok(())
}

fn get_metadata() -> Result<()> {
    let metadata = MetadataCommand::new()
        .no_deps()
        .exec()
        .context("Can't get cargo metadata")?;

    // println!("{:#?}", metadata);

    if let Err(_) = NAME.set(metadata.packages[0].name.to_string()) {
        anyhow::bail!("Fail to init NAME");
    }

    if let Err(_) = VERSION.set(metadata.packages[0].version.to_string()) {
        anyhow::bail!("Fail to init VERSION");
    }

    let sh = Shell::new()?;
    let host_name = cmd!(sh, "rustc -vV")
        .read()?
        .lines()
        .find(|line| line.starts_with("host:"))
        .map(|line| line["host:".len()..].trim().to_string())
        .context("Can't get info of host")?;

    if let Err(_) = HOST_NAME.set(host_name) {
        anyhow::bail!("Fail to init HOST_NAME");
    }

    Ok(())
}

fn get_name() -> &'static str {
    match NAME.get() {
        Some(x) => x,
        None => "",
    }
}

fn get_version() -> &'static str {
    match VERSION.get() {
        Some(x) => x,
        None => "",
    }
}

fn get_host_name() -> &'static str {
    match HOST_NAME.get() {
        Some(x) => x,
        None => "",
    }
}

fn get_target_folder_name() -> String {
    format!("{}-v{}-{}", get_name(), get_version(), get_host_name())
}

fn release() -> Result<()> {
    println!("Building {}...", get_name());

    let sh = Shell::new()?;

    cmd!(sh, "cargo build --release").run()?;

    sh.create_dir(format!("dist/{}", get_target_folder_name()))?;
    sh.create_dir(format!("dist/{}/logs", get_target_folder_name()))?;

    let is_windows = get_host_name().contains("windows");

    if is_windows {
        sh.copy_file(
            format!("./target/release/{}.exe", get_name()),
            format!("./dist/{}/{}.exe", get_target_folder_name(), get_name(),),
        )?;
    } else {
        sh.copy_file(
            format!("./target/release/{}", get_name()),
            format!("./dist/{}/{}.exe", get_target_folder_name(), get_name(),),
        )?;
    }

    Ok(())
}

fn package() -> Result<()> {
    let file = File::create(format!("./dist/{}.zip", get_target_folder_name()))?;
    let mut zip = ZipWriter::new(file);

    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    let src_dir = format!("./dist/{}/", get_target_folder_name());
    let src_dir: &Path = Path::new(src_dir.as_str()).as_ref();

    for entry in WalkDir::new(src_dir).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        let src_dir_parent: &Path = src_dir.parent().context("Can't get parent")?;
        let name = path.strip_prefix(src_dir_parent)?;

        if path.is_file() {
            let name_str = name.to_str().context("There are invariant UTF-8 char")?;
            zip.start_file(name_str, options)?;
            let mut f = File::open(path)?;
            std::io::copy(&mut f, &mut zip)?;
        }
    }

    zip.finish()?;

    Ok(())
}
