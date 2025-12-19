#![allow(non_snake_case)]

use clap::Parser;
use color_eyre::eyre::{Context, ContextCompat, Result, eyre};
use either::Either;
use indicatif::ProgressBar;
use isahc::{AsyncReadResponseExt, ReadResponseExt, ResponseExt};
use serde::Deserialize;
use std::{
    fs::File,
    io::Cursor,
    path::{Path, PathBuf},
};
use zip::ZipArchive;

#[derive(Debug, Deserialize)]
struct GoogleFontFileRef {
    filename: PathBuf,
    url: String,
    // don't care about date
}

#[derive(Debug, Deserialize)]
struct Manifest {
    // files: Vec<GoogleFontFile>,
    fileRefs: Vec<GoogleFontFileRef>,
}

#[derive(Debug, Deserialize)]
struct List {
    // zipName: PathBuf,
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

fn get_google_font(specimen_url: &str, extract_root_dir: &Path) -> Result<()> {
    use futures_concurrency::prelude::*;

    println!("detected '{}' as a Google Fonts URL!", &specimen_url);

    let font_name = specimen_url
        .split("/")
        .last()
        .context("failed resolving font name from URL")?;

    smol::block_on(async {
        let list_url = format!(
            "https://fonts.google.com/download/list?family={}",
            font_name
        );
        let mut list_response = isahc::get_async(list_url).await?;
        if !list_response.status().is_success() {
            return Err(eyre!("bad request, got {}", list_response.status()));
        }

        println!("downloading font '{}'...", font_name);

        let list_response_text = list_response
            .text()
            .await?
            .split_once("\n")
            .unwrap()
            .1
            .to_string();

        let list: List = serde_json::from_str(&list_response_text).unwrap();

        let progress_bar = ProgressBar::new(list.manifest.fileRefs.len() as u64);

        list.manifest
            .fileRefs
            .into_co_stream()
            .try_for_each(async |fileref| -> Result<()> {
                let contents = isahc::get_async(fileref.url).await?.bytes().await?;
                let path = extract_root_dir.join(font_name).join(fileref.filename);
                smol::fs::create_dir_all(path.parent().unwrap()).await?;
                smol::fs::write(path, contents).await?;
                progress_bar.inc(1);
                Ok(())
            })
            .await?;
        println!("successfully downloaded and installed '{}'!", font_name);
        Ok(())
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

    if args
        .font_location
        .starts_with("https://fonts.google.com/specimen/")
    {
        return get_google_font(&args.font_location, &extract_root_dir);
    }

    let (font_name, font_reader): (String, _) = if args.font_location.starts_with("http") {
        let mut response =
            isahc::get(&args.font_location).context("unable to download font archive")?;
        let name = if let Some(uri) = response.effective_uri() {
            uri.to_string()
        } else {
            args.font_location.to_string()
        }
        .replace("/", "-");
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

    println!("fonts installed to {:?}!", extract_directory);

    Ok(())
}
