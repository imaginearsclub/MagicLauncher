//! pack-publisher: one-step modpack publishing for the pack owner (command line).
//!
//! Double-click it (or run it from a terminal) and it:
//! 1. finds the pack instance (asks once, then remembers),
//! 2. builds the pack and compares it with the live one,
//! 3. suggests the next SemVer version (removed mods → MAJOR, added → MINOR, else PATCH),
//! 4. after a y/N confirmation uploads only changed files, then pack.json, then
//!    clears the CloudFront cache.
//!
//! AWS keys are asked for on first run and kept in the OS keychain. The work itself is in
//! the library (`lib.rs`), shared with the Imaginears PackPublisher window.

use clap::Parser;
use pack_builder::instances;
use pack_publisher::{self as publisher, BoxError, Credentials, PrepareOptions, Target};
use std::io::{BufRead, Write};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    version,
    about = "Build the Imaginears Magic Pack and publish it to players"
)]
struct Args {
    /// Version to publish. Suggested automatically if omitted.
    #[arg(long = "pack-version")]
    pack_version: Option<semver::Version>,
    /// pack instance folder. Asked for (and remembered) if omitted.
    #[arg(long)]
    instance: Option<PathBuf>,
    /// Show what would change, upload nothing.
    #[arg(long)]
    dry_run: bool,
    /// Don't ask for confirmation (for scripts).
    #[arg(long, short)]
    yes: bool,
    /// Re-enter the AWS access keys.
    #[arg(long)]
    setup: bool,
    /// Re-upload every self-hosted file, not only changed ones (repairs a broken bucket).
    #[arg(long)]
    reupload_all: bool,
    /// Maintenance mode instead of publishing: `--maintenance "Back at 18:00"` disables Play
    /// in every launcher with that message; `--maintenance off` re-enables it.
    #[arg(long, value_name = "MESSAGE|off")]
    maintenance: Option<String>,
    /// Recommended max memory for the pack in MiB (players on "automatic" get this, capped
    /// at half their RAM). Default 4096; large packs need more.
    #[arg(long, value_name = "MiB")]
    memory: Option<u32>,
    /// Use this Minecraft version instead of the instance's (needed for plain folders).
    #[arg(long)]
    minecraft: Option<String>,
    /// Use this Fabric Loader version instead of the instance's (e.g. when the instance is
    /// older than its mods require; needed for plain folders).
    #[arg(long)]
    fabric: Option<String>,
    /// A note for players shown with this update in the launcher ("What's new").
    /// Asked for interactively if omitted.
    #[arg(long)]
    notes: Option<String>,
    /// Publish even if nothing changed or the version isn't higher than the live one.
    #[arg(long)]
    force: bool,
    /// Publish a test-channel pack (`beta` or `alpha`) instead of the Release pack.
    /// It goes to `<base-url>/<channel>/pack.json` with its own files, and launchers on
    /// that channel use it instead of the Release pack.
    #[arg(long, value_parser = ["beta", "alpha"])]
    channel: Option<String>,
    #[arg(long, default_value = pack_publisher::DEFAULT_BASE_URL)]
    base_url: String,
    #[arg(long, default_value = pack_publisher::DEFAULT_BUCKET)]
    bucket: String,
    #[arg(long, default_value = pack_publisher::DEFAULT_PREFIX)]
    prefix: String,
    #[arg(long, default_value = pack_publisher::DEFAULT_DISTRIBUTION_ID)]
    distribution_id: String,
    #[arg(long, default_value = pack_publisher::DEFAULT_REGION)]
    region: String,
    /// List client-only mods that could become optional (for magic-pack.toml), then exit.
    /// Uploads nothing.
    #[arg(long)]
    suggest_optional: bool,
    /// Exit without "Press Enter to close".
    #[arg(long)]
    no_pause: bool,
}

impl Args {
    fn target(&self) -> Target {
        Target {
            base_url: self.base_url.clone(),
            bucket: self.bucket.clone(),
            prefix: self.prefix.clone(),
            distribution_id: self.distribution_id.clone(),
            region: self.region.clone(),
            channel: self.channel.clone(),
        }
    }
}

#[tokio::main]
async fn main() {
    let args = Args::parse();
    let no_pause = args.no_pause || args.yes;
    println!(
        "Imaginears Club pack publisher {}\n",
        env!("CARGO_PKG_VERSION")
    );
    let code = match run(args).await {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("\nError: {e}");
            1
        }
    };
    if !no_pause {
        prompt("\nPress Enter to close.");
    }
    std::process::exit(code);
}

async fn run(args: Args) -> Result<(), BoxError> {
    let target = args.target();
    if let Some(m) = args.maintenance.clone() {
        return set_maintenance(&args, &target, &m).await;
    }
    let channel = args.channel.clone();
    if let Some(ch) = &channel {
        // Own pack.json and own files: a test pack never overwrites Release files.
        println!("Channel: {ch} (launchers on {ch} use this pack; Release is untouched)");
    }

    let instance = choose_instance(args.instance.clone(), channel.as_deref())?;
    println!("Instance: {}\n", instance.display());
    if args.suggest_optional {
        let (cfg, src) = pack_builder::features::Config::for_instance(&instance)?;
        println!("Using the {src} {}.\n", pack_builder::features::FILE_NAME);
        let mods = pack_builder::suggest::scan_mods(&instance);
        println!("Scanned {} mods.\n", mods.len());
        let found = pack_builder::suggest::suggest(&mods, &cfg);
        print!("{}", pack_builder::suggest::report(&found));
        return Ok(());
    }

    let opts = PrepareOptions {
        minecraft: args.minecraft.clone(),
        fabric: args.fabric.clone(),
        memory_mb: args.memory,
        reupload_all: args.reupload_all,
    };
    let mut p = publisher::prepare(&target, &instance, &opts, publisher::stdout_log()).await?;

    match &p.diff {
        Some(d) if !d.is_empty() => {
            println!("\nChanges compared with the live pack:");
            for line in publisher::diff_lines(d) {
                println!("{line}");
            }
        }
        Some(_) => {}
        None => println!("\nFirst publish: {} files.", p.built.manifest.files.len()),
    }
    if p.unchanged() && !args.force && !args.reupload_all {
        println!("\nNothing changed: the live pack already matches this instance.");
        return Ok(());
    }

    let version = match args.pack_version.clone() {
        Some(v) => v,
        None if args.yes || args.dry_run => p.suggested.clone(),
        None => ask_version(&p.suggested)?,
    };
    // "What's new" for players: the diff above plus an optional note.
    let notes = match args.notes.clone() {
        Some(n) => Some(n),
        None if args.yes || args.dry_run => None,
        None => Some(prompt(
            "\nNote for players about this update (optional, Enter to skip):",
        )),
    };
    publisher::finalize(&mut p, version.clone(), notes, args.force)?;

    println!(
        "\nPack {version}: {} files ({} self-hosted mods). Uploading {} changed file(s), {:.1} MB.",
        p.built.manifest.files.len(),
        p.built.self_hosted_mods().count(),
        p.uploads.len(),
        p.upload_bytes as f64 / 1_048_576.0
    );
    if !p.new_self_hosted.is_empty() {
        println!(
            "\nNew mods that will be hosted on S3 (not on Modrinth). Check their licenses allow it:"
        );
        for m in &p.new_self_hosted {
            println!("  - {m}");
        }
    }

    if args.dry_run {
        println!("\nDry run: nothing uploaded.");
        return Ok(());
    }
    let creds = credentials(args.setup)?;
    if !args.yes && !confirm(&format!("\nPublish pack {version} to players?"))? {
        println!("Cancelled; nothing uploaded.");
        return Ok(());
    }

    publisher::publish(
        &target,
        &p,
        &creds,
        args.force,
        publisher::stdout_log(),
        &|n, total| {
            print!("\rUploading files: {n}/{total}   ");
            std::io::stdout().flush().ok();
            if n == total {
                println!();
            }
        },
    )
    .await?;
    println!("\nPublished pack {version}. Players get it the next time they press Play.");
    Ok(())
}

/// `--maintenance "message"` / `--maintenance off`.
async fn set_maintenance(args: &Args, target: &Target, value: &str) -> Result<(), BoxError> {
    let on = !value.trim().eq_ignore_ascii_case("off");
    let message = on
        .then(|| value.trim().to_string())
        .filter(|m| !m.is_empty());
    if on {
        println!(
            "Maintenance ON: Play will be disabled for players with the message:\n  {}",
            message.as_deref().unwrap_or("(no message)")
        );
    } else {
        println!("Maintenance OFF: players can play again.");
    }
    if !args.yes && !confirm("Continue?")? {
        println!("Cancelled.");
        return Ok(());
    }
    let creds = credentials(args.setup)?;
    publisher::set_maintenance(target, on, message, &creds).await?;
    println!("Done. Launchers pick it up within about a minute.");
    Ok(())
}

// ---------- prompts ----------

fn prompt(msg: &str) -> String {
    print!("{msg} ");
    std::io::stdout().flush().ok();
    let mut s = String::new();
    std::io::stdin().lock().read_line(&mut s).ok();
    s.trim().to_string()
}

fn confirm(msg: &str) -> Result<bool, BoxError> {
    Ok(matches!(
        prompt(&format!("{msg} [y/N]")).to_lowercase().as_str(),
        "y" | "yes"
    ))
}

fn ask_version(suggested: &semver::Version) -> Result<semver::Version, BoxError> {
    loop {
        let s = prompt(&format!("\nVersion to publish [{suggested}]:"));
        if s.is_empty() {
            return Ok(suggested.clone());
        }
        match semver::Version::parse(s.trim_start_matches('v')) {
            Ok(v) => return Ok(v),
            Err(_) => println!("  Use the form MAJOR.MINOR.PATCH, e.g. {suggested}."),
        }
    }
}

// ---------- instance choice ----------

fn choose_instance(arg: Option<PathBuf>, channel: Option<&str>) -> Result<PathBuf, BoxError> {
    if let Some(p) = arg {
        instances::read(&p).ok_or_else(|| {
            format!(
                "{} is not a pack instance (Prism Launcher, Modrinth App, CurseForge, \
                 or a folder with mods/)",
                p.display()
            )
        })?;
        return Ok(p);
    }
    if let Some(p) = publisher::remembered_instance(channel) {
        return Ok(p);
    }

    let found = instances::list_default();
    let chosen = if found.is_empty() {
        println!("No Prism Launcher, Modrinth App or pack instances found in the usual places.");
        let p = PathBuf::from(
            prompt("Paste the path of the pack's instance folder:").trim_matches('"'),
        );
        instances::read(&p)
            .ok_or_else(|| format!("{} is not a pack instance", p.display()))?
            .path
    } else {
        match channel {
            Some(ch) => println!("Which instance is the Imaginears Magic Pack ({ch})?"),
            None => println!("Which instance is the Imaginears Magic Pack?"),
        }
        for (i, inst) in found.iter().enumerate() {
            println!(
                "  {}. {}  ({}, Minecraft {}, {})",
                i + 1,
                inst.name,
                inst.kind.label(),
                if inst.minecraft.is_empty() {
                    "?"
                } else {
                    &inst.minecraft
                },
                inst.loader.as_deref().unwrap_or("no loader")
            );
        }
        loop {
            let s = prompt(&format!("Number [1-{}]:", found.len()));
            match s.parse::<usize>() {
                Ok(n) if (1..=found.len()).contains(&n) => break found[n - 1].path.clone(),
                _ => println!("  Type a number from the list."),
            }
        }
    };
    publisher::save_instance(&chosen, channel);
    println!("Saved. (Run with --instance <folder> to use a different one.)");
    Ok(chosen)
}

// ---------- AWS keys ----------

fn credentials(force_setup: bool) -> Result<Credentials, BoxError> {
    // Scripts/CI: the standard AWS environment variables win over the stored key.
    if !force_setup {
        if let Some(c) = publisher::env_credentials().or_else(publisher::stored_credentials) {
            return Ok(c);
        }
    }
    println!("First-time setup: enter the AWS access key for publishing the pack.");
    println!("(It is stored in this computer's credential manager, not in a file.)\n");
    let id = prompt("Access key ID:");
    let secret = rpassword::prompt_password("Secret access key (hidden): ")?;
    let c = publisher::store_credentials(&id, &secret)?;
    println!("Saved.\n");
    Ok(c)
}
