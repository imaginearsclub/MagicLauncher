//! pack-builder: turn a modpack folder (CurseForge instance) or zip into
//! `<out>/pack.json` plus `<out>/files/…` holding everything that has to be
//! self-hosted. Jars that Modrinth knows by hash point at Modrinth's CDN instead.
//!
//! It only ever READS the input; nothing in the source instance is modified.
//! To build and upload in one go, use `pack-publisher` instead.

use clap::Parser;
use pack_builder::{
    build, BoxError, BuildOptions, DEFAULT_DEFAULT_PREFIX, DEFAULT_EXCLUDE, DEFAULT_INCLUDE,
    DEFAULT_MANAGED_PREFIX,
};
use std::path::PathBuf;

#[derive(Parser)]
#[command(version, about)]
struct Args {
    /// CurseForge instance folder, or a .zip of one.
    #[arg(long)]
    input: PathBuf,
    /// Output folder: pack.json and files/ to upload.
    #[arg(long, default_value = "dist-pack")]
    out: PathBuf,
    /// The modpack's SemVer, e.g. 1.0.0. Bump MAJOR when players must reinstall/worlds break.
    #[arg(long)]
    pack_version: semver::Version,
    /// Public base URL where the out folder will be hosted (pack.json lives at <base-url>/pack.json).
    #[arg(long)]
    base_url: String,
    #[arg(long, default_value = "imaginears-magic-pack")]
    id: String,
    #[arg(long, default_value = "Imaginears Magic Pack")]
    name: String,
    /// Minecraft version (auto-detected from the Prism / CurseForge / Modrinth instance).
    #[arg(long)]
    minecraft: Option<String>,
    /// Fabric Loader version (auto-detected from the instance).
    #[arg(long)]
    fabric: Option<String>,
    /// Recommended max heap in MiB.
    #[arg(long, default_value_t = 4096)]
    memory: u32,
    /// Folders/files to include, relative to the instance.
    #[arg(long, value_delimiter = ',', default_values = DEFAULT_INCLUDE)]
    include: Vec<String>,
    /// Path prefixes installed once and then left to the player.
    #[arg(long, value_delimiter = ',', default_values = DEFAULT_DEFAULT_PREFIX)]
    default_prefix: Vec<String>,
    /// Path prefixes always kept in sync; the longest matching prefix wins over --default-prefix.
    #[arg(long, value_delimiter = ',', default_values = DEFAULT_MANAGED_PREFIX)]
    managed_prefix: Vec<String>,
    /// Path prefixes left out even inside included folders.
    #[arg(long, value_delimiter = ',', default_values = DEFAULT_EXCLUDE)]
    exclude: Vec<String>,
    /// Don't look jars up on Modrinth; self-host everything.
    #[arg(long)]
    no_modrinth: bool,
}

#[tokio::main]
async fn main() -> Result<(), BoxError> {
    let args = Args::parse();
    let out = args.out.clone();
    std::fs::create_dir_all(&out)?;

    let extracted = out.join(".input");
    let root = if args.input.is_file() {
        let _ = std::fs::remove_dir_all(&extracted);
        println!("Extracting {} …", args.input.display());
        zip::ZipArchive::new(std::fs::File::open(&args.input)?)?.extract(&extracted)?;
        extracted.clone()
    } else {
        args.input.clone()
    };

    let mut opts = BuildOptions::new(root, args.pack_version, args.base_url.clone());
    opts.id = args.id;
    opts.name = args.name;
    opts.minecraft = args.minecraft;
    opts.fabric = args.fabric;
    opts.memory_mb = args.memory;
    opts.include = args.include;
    opts.default_prefix = args.default_prefix;
    opts.managed_prefix = args.managed_prefix;
    opts.exclude = args.exclude;
    opts.use_modrinth = !args.no_modrinth;
    let built = build(opts).await?;

    let files_dir = out.join("files");
    let _ = std::fs::remove_dir_all(&files_dir);
    for (rel, src) in &built.self_hosted {
        let dest = files_dir.join(rel);
        std::fs::create_dir_all(dest.parent().expect("has parent"))?;
        std::fs::copy(src, &dest)?;
    }
    let manifest = &built.manifest;
    std::fs::write(out.join("pack.json"), serde_json::to_vec_pretty(manifest)?)?;
    let _ = std::fs::remove_dir_all(&extracted);

    println!(
        "\nWrote {} (pack {}, MC {}, Fabric Loader {})",
        out.join("pack.json").display(),
        manifest.version,
        manifest.minecraft,
        manifest.loader.version
    );
    println!(
        "  {} files total, {} from Modrinth's CDN, {} self-hosted in {}",
        manifest.files.len(),
        manifest.files.len() - built.self_hosted.len(),
        built.self_hosted.len(),
        files_dir.display()
    );
    let mods: Vec<_> = built.self_hosted_mods().collect();
    if !mods.is_empty() {
        println!("\nThese mods are not on Modrinth and will be self-hosted. Check each license allows redistribution:");
        for j in mods {
            println!("  - {j}");
        }
    }
    println!(
        "\nUpload the contents of {} to {}/",
        out.display(),
        args.base_url.trim_end_matches('/')
    );
    Ok(())
}
