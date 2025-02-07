use ansi_term::{
    Color::{Green, Red},
    Style,
};
use anyhow::{anyhow, Context, Result};
use clap::Parser;
use inquire::Confirm;
use std::{
    fs::{read_link, remove_file},
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

    if let Some(dst_str) = dst.canonicalize()?.as_path().to_str() {
        if !args.force && dst_str == "/" {
            eprintln!("This will recursively convert every absolute symlink in your root directory to a relative one");
            eprintln!("You probably don't want to do this. If you are sure you want to do this, pass the -f option to override this check");
            exit(1);
        }
    } else {
        return Err(anyhow!("Cannot check destination directory"));
    }

    let bold = Style::new().bold();
    println!(
        "{}{}: ",
        bold.paint("The following operations will occur and "),
        Red.bold().paint("are possibly destructive")
    );
    println!("    - Any files in the destination directory may be overwritten");
    println!("    - All symlinks in the destination directory will be converted to their relative equivalents");
    println!();
    println!(
        "{}",
        Style::new()
            .bold()
            .paint("Using the following directories: ")
    );
    println!("{} {}", Green.bold().paint("Source:"), src.display());
    println!("{} {}", Green.bold().paint("Destination:"), dst.display());
    println!();
    let ans = Confirm::new("Continue?").with_default(false).prompt()?;
    if !ans {
        eprintln!("Aborting");
        exit(0);
    }

    make_relative(dst)?;
    Ok(())
}

fn make_relative(sysroot_dir: PathBuf) -> Result<()> {
    for entry in WalkDir::new(&sysroot_dir) {
        let entry = entry?;
        if entry.path_is_symlink() {
            let target = read_link(entry.path())?;
            if target.is_absolute() {
                let real_path = sysroot_dir.join(target.strip_prefix("/")?);
                dbg!(entry.path());
                dbg!(&real_path);
                let rel_path = pathdiff::diff_paths(
                    real_path.parent().unwrap(),
                    entry.path().parent().unwrap(),
                )
                .ok_or_else(|| {
                    anyhow!("Failed to resolve absolute symlink target to a relative one",)
                })?
                .join(real_path.file_name().unwrap());
                remove_file(entry.path())?;
                symlink(rel_path, entry.path())?;
            }
        }
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

    /// Force re-symlinking
    #[arg(short, long)]
    force: bool,
}
