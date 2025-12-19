#![allow(non_snake_case)]

use clap::Parser;
use color_eyre::eyre::{Context, ContextCompat, Result, eyre};
use either::Either;
use serde::Deserialize;
use std::{
    fs::File,
    io::Cursor,
    path::{Path, PathBuf},
};
use zip::ZipArchive;

#[derive(Deserialize)]
struct GoogleFontFile {
    filename: PathBuf,
    contents: String,
}

#[derive(Deserialize)]
struct GoogleFontFileRef {
    filename: PathBuf,
    url: String,
    // don't care about date
}

#[derive(Deserialize)]
struct Manifest {
    files: Vec<GoogleFontFile>,
    fileRefs: Vec<GoogleFontFileRef>,
}

#[derive(Deserialize)]
struct ListResponse {
    zipName: PathBuf,
    manifest: Manifest,
}

#[derive(Parser)]
struct Args {
    /// local file path or remote URL of archive containing font
    font_location: String,

    /// only try installing system-wide (i.e. in /usr/share/fonts/)
    #[arg(long)]
    only_system: bool,

    /// only try installing for current user (i.e. in $XDG_DATA_HOME/fonts/)
    #[arg(long)]
    only_user: bool,
}

fn get_google_font(url: &str, extract_root_dir: &Path) -> Result<()> {
    use futures_concurrency::prelude::*;

    let client = reqwest::Client::new();

    smol::block_on(async {
        let list_response: ListResponse = client.get(url).send().await?.json().await?;

        list_response
            .manifest
            .fileRefs
            .into_co_stream()
            .try_for_each(async |fileref| -> Result<()> {
                let contents = client.get(fileref.url).send().await?.bytes().await?;
                smol::fs::write(extract_root_dir.join(fileref.filename), contents).await?;
                Ok(())
            })
            .await
    })
}

fn has_write_permissions(path: impl AsRef<Path>) -> bool {
    File::options().write(true).open(path).is_ok()
}

fn main() -> Result<()> {
    let args = Args::parse();
    if args.only_user && args.only_system {
        return Err(eyre!(
            "at most one of --only-user and --only-system can be given"
        ));
    }

    const SYSTEM_FONT_DIR: &str = "/usr/share/fonts";

    let extract_root_dir = if !args.only_user && has_write_permissions(SYSTEM_FONT_DIR) {
        SYSTEM_FONT_DIR.into()
    } else if !args.only_system {
        dirs::font_dir().context("unable to determine user font dir")?
    } else {
        return Err(eyre!(
            "no suitable font installation directory found; try running without --only-system or --only-user"
        ));
    };

    if args.font_location.starts_with("https://fonts.google.com") {
        return get_google_font(&args.font_location, &extract_root_dir);
    }

    let (font_name, font_reader): (String, _) = if args.font_location.starts_with("http") {
        let response = reqwest::blocking::get(&args.font_location)
            .context("unable to download font archive")?;
        let name = response.url().to_string().replace("/", "-");
        let reader = Either::Left(Cursor::new(response.bytes()?));
        (name, reader)
    } else {
        let font_path = Path::new(&args.font_location);
        let name = font_path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let reader = Either::Right(
            File::open(font_path).context("unable to open font archive for reading")?,
        );
        (name, reader)
    };
    let mut archive = ZipArchive::new(font_reader)?;

    let extract_directory = extract_root_dir.join(font_name);

    archive
        .extract(&extract_directory)
        .context("failed extracting to font directory")?;

    println!("Fonts installed to {:?}", extract_directory);

    Ok(())
}
