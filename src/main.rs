use ansi_term::{
    Color::{Cyan, Green, Red},
    Style,
};
use anyhow::{anyhow, Context, Result};
use clap::Parser;
use core::panic;
use inquire::Confirm;
use make_sysroot::CopyBuilder;
use serde::Deserialize;
use std::{
    fmt::{Debug, Display},
    fs::{create_dir_all, read_link, read_to_string, remove_dir_all, remove_file},
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
    describe(src.display(), dst.display(), &config);
    if !query("Continue?")? {
        eprintln!("Aborting");
        exit(0);
    }

    copy(&src, &dst, &config)?;
    create_explicit_symlinks(&dst, config.link)?;
    make_relative(&dst)?;
    Ok(())
}

fn create_explicit_symlinks(dst: &PathBuf, links: Vec<Link>) -> Result<()> {
    for link in links {
        if link.link.is_absolute() {
            let abs_link = dst.join(link.link.strip_prefix("/")?);
            if abs_link.symlink_metadata().is_ok() {
                println!(
                    "{}",
                    Red.bold().paint(format!(
                        "File {} already exists but was specified for symlinking",
                        &abs_link.to_string_lossy()
                    ))
                );
                if query("Replace it?")? {
                    remove_file(&abs_link)?;
                } else {
                    println!("{}", Red.bold().paint("Skipping..."));
                    continue;
                }
            }
            create_dir_all(&abs_link.parent().unwrap())?;
            symlink(link.target, &abs_link)?;
        }
    }
    Ok(())
}

fn copy(src: &PathBuf, dst: &PathBuf, config: &Config) -> Result<()> {
    let mut copier = CopyBuilder::new(&src, &dst).overwrite_if_newer(true);
    for path in config.include.iter() {
        copier = copier.with_include_path(
            src.join(path.strip_prefix("/").with_context(|| {
                Red.bold().paint(format!(
                    "The provided include path {} is not absolute",
                    path.to_string_lossy()
                ))
            })?)
            .to_str()
            .ok_or_else(|| anyhow!("Failed to parse an include path"))?,
        );
    }
    for path in config.exclude.iter() {
        copier = copier.with_exclude_path(
            src.join(path.strip_prefix("/").with_context(|| {
                Red.bold().paint(format!(
                    "The provided exclude path {} is not absolute",
                    path.to_string_lossy()
                ))
            })?)
            .to_str()
            .ok_or_else(|| anyhow!("Failed to parse an exclude path"))?,
        );
    }
    copier.run()?;
    // Clean up some empty parent directories the copy proccess leaves behind from exlcuded files
    for path in config.exclude.iter() {
        let abs_path = dst.join(path.strip_prefix("/")?);
        if abs_path.exists() {
            remove_dir_all(&abs_path).context(abs_path.to_string_lossy().into_owned())?;
        }
    }
    Ok(())
}

fn make_relative(sysroot_dir: &PathBuf) -> Result<()> {
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

fn describe<T: Display>(src: T, dst: T, config: &Config) {
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
    if config.link.len() > 0 {
        println!("{}", bold.paint("The following symlinks will be created: "));
        for link in config.link.iter() {
            println!(
                "{} -> {}",
                Cyan.paint(link.link.to_string_lossy()),
                link.target.to_string_lossy()
            )
        }
    }
}

fn query<T: Display>(prompt: T) -> Result<bool> {
    Confirm::new(prompt.to_string().as_str())
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
    link: Vec<Link>,
}

#[derive(Deserialize, Debug)]
struct Link {
    link: PathBuf,
    target: PathBuf,
}
