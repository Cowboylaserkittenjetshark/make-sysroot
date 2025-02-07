use ansi_term::{
    Color::{Green, Red},
    Style,
};
use anyhow::{anyhow, Context, Result};
use clap::Parser;
use dircpy::CopyBuilder;
use inquire::Confirm;
use serde::Deserialize;
use std::{
    fmt::Display,
    fs::{read_link, read_to_string, remove_file},
    os::unix::fs::symlink,
    path::{absolute, PathBuf},
    process::exit,
};
use walkdir::WalkDir;

fn main() -> Result<()> {
    let args = Args::parse();
    let src = absolute(args.source).context("Failed to convert source path to an absolute path")?;
    let dst = absolute(args.destination)
        .context("Failed to convert destination path to an absolute path")?;

    if src.is_file() {
        return Err(anyhow!("source should be a directory but is a file"));
    }
    if dst.is_file() {
        return Err(anyhow!("destination should be a directory but is a file"));
    }
    if !args.force {
        check_dst(&dst)?;
    }

    let config_str = read_to_string(args.config).context("Config file not found")?;
    let config: Config = toml::from_str(&config_str)?;

    if !confirm(src.display(), dst.display(), config)? {
        eprintln!("Aborting");
        exit(0);
    }
    // CopyBuilder::new(&src, &dst).overwrite_if_newer(true).with_exclude_filter(f);
    make_relative(dst)?;
    Ok(())
}

fn make_relative(sysroot_dir: PathBuf) -> Result<()> {
    // Recursively walk through all directories in the sysroot
    for entry in WalkDir::new(&sysroot_dir) {
        let entry = entry?;
        if entry.path_is_symlink() {
            // Get the target of the symlink
            let target = read_link(entry.path())?;
            // Only operate on links who's target is absolute
            if target.is_absolute() {
                let real_path = sysroot_dir.join(target.strip_prefix("/")?);
                // Get target path relative to the entry path
                let rel_path = pathdiff::diff_paths(
                    real_path.parent().unwrap(),
                    entry.path().parent().unwrap(),
                )
                .ok_or_else(|| {
                    anyhow!("Failed to resolve absolute symlink target to a relative one",)
                })?
                .join(real_path.file_name().unwrap()); // Preserve the filename of the original target
                remove_file(entry.path())?;
                symlink(rel_path, entry.path())?;
            }
        }
    }
    Ok(())
}

fn confirm<T: Display>(src: T, dst: T, config: Config) -> Result<bool> {
    let bold = Style::new().bold();
    println!(
        "{}{}: ",
        bold.paint("The following operations will occur and "),
        Red.bold().paint("are possibly destructive")
    );
    println!("    - Any files in the destination directory may be overwritten");
    println!("    - All symlinks in the destination directory will be converted to their relative equivalents");
    println!();
    println!("{}", bold.paint("Using the following directories: "));
    println!("{} {}", Green.bold().paint("Source:"), src);
    println!("{} {}", Green.bold().paint("Destination:"), dst);
    println!();
    println!(
        "{} {} {} {}{}",
        bold.paint("The following paths will be copied to the destination directory,"),
        Green.bold().paint("including"),
        bold.paint("and"),
        Red.bold().paint("excluding"),
        bold.paint(":")
    );
    let mut includes = config.include.clone();
    let mut excludes = config.exclude.clone();
    includes.append(&mut excludes);
    let mut combined_paths = includes;
    combined_paths.sort_unstable();
    for path in combined_paths {
        if config.include.contains(&path) {
            println!(
                "{} {}",
                Green.paint("+"),
                Green.paint(path.to_string_lossy())
            );
        } else if config.exclude.contains(&path) {
            println!("{} {}", Red.paint("-"), Red.paint(path.to_string_lossy()));
        } else {
            println!("  {}", path.to_string_lossy());
        }
    }

    Confirm::new("Continue?")
        .with_default(false)
        .prompt()
        .context("Context")
}

fn check_dst(dst: &PathBuf) -> Result<()> {
    if let Some(dst_str) = dst.canonicalize()?.as_path().to_str() {
        if dst_str == "/" {
            eprintln!("This will recursively convert every absolute symlink in your root directory to a relative one");
            eprintln!("You probably don't want to do this. If you are sure you want to do this, pass the -f option to override this check");
            exit(1);
        }
    } else {
        return Err(anyhow!("Cannot check destination directory"));
    }
    Ok(())
}

/// A tool for building sysroots for cross compilation
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Source directory to build the sysroot from
    #[arg(short, long)]
    source: PathBuf,

    /// Destination directory to build the sysroot in
    #[arg(short, long)]
    destination: PathBuf,

    /// Path to the configuration file
    #[arg(short, long, default_value = "make-sysroot.toml")]
    config: PathBuf,

    /// Force re-symlinking
    #[arg(short, long)]
    force: bool,
}

#[derive(Deserialize)]
struct Config {
    include: Vec<PathBuf>,
    exclude: Vec<PathBuf>,
}
