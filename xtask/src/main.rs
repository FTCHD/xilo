use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use std::process::Command as Process;

mod import;

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use serde::Deserialize;

#[derive(Parser)]
#[command(name = "xtask")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Copy Anker assets listed in assets/manifest.toml into assets/vendor.
    Assets {
        #[command(subcommand)]
        action: AssetAction,
    },
    /// Generate model definitions from a reference project.
    Import {
        #[command(subcommand)]
        source: ImportSource,
    },
    /// Fail if private files are tracked by git.
    CheckPublic,
    /// Build a macOS app bundle in dist/.
    Bundle {
        #[arg(long)]
        release: bool,
    },
}

#[derive(Subcommand)]
enum AssetAction {
    Extract {
        /// Folder holding the unpacked .xapk (the inner .apk files).
        #[arg(long, default_value = ".research/anker-apk/xapk")]
        apks: PathBuf,
    },
    Check,
}

#[derive(Subcommand)]
enum ImportSource {
    /// Field and command maps from anker-solix-api.
    SolixApi {
        #[arg(
            long,
            default_value = ".research/anker-solix-api/repo/src/anker_solix_api"
        )]
        src: PathBuf,
    },
}

#[derive(Deserialize)]
struct Manifest {
    apks: HashMap<String, String>,
    asset: Vec<Asset>,
}

#[derive(Deserialize)]
struct Asset {
    key: String,
    apk: String,
    path: String,
}

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_default()
}

fn manifest() -> Result<Manifest> {
    let text = fs::read_to_string(root().join("assets/manifest.toml"))?;
    Ok(toml::from_str(&text)?)
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Assets {
            action: AssetAction::Extract { apks },
        } => extract(&root().join(apks)),
        Command::Assets {
            action: AssetAction::Check,
        } => check(),
        Command::Import {
            source: ImportSource::SolixApi { src },
        } => import::run(&root(), &root().join(src)),
        Command::CheckPublic => check_public(),
        Command::Bundle { release } => bundle(release),
    }
}

fn extract(apks: &Path) -> Result<()> {
    let manifest = manifest()?;
    let vendor = root().join("assets/vendor");
    let mut archives = HashMap::new();
    for (name, file) in &manifest.apks {
        let path = apks.join(file);
        let archive = zip::ZipArchive::new(
            File::open(&path).with_context(|| format!("open {}", path.display()))?,
        )?;
        archives.insert(name.clone(), archive);
    }
    let mut copied = 0;
    for asset in &manifest.asset {
        let Some(archive) = archives.get_mut(&asset.apk) else {
            bail!("unknown apk {} for {}", asset.apk, asset.key);
        };
        let mut bytes = Vec::new();
        match archive.by_name(&asset.path) {
            Ok(mut entry) => {
                entry.read_to_end(&mut bytes)?;
            }
            Err(_) => {
                eprintln!("missing in apk: {} ({})", asset.key, asset.path);
                continue;
            }
        }
        let ext = Path::new(&asset.path)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("bin");
        if ext == "zip" {
            split_lottie(&bytes, &vendor.join(&asset.key))?;
        } else {
            let out = vendor.join(format!("{}.{ext}", asset.key));
            if let Some(dir) = out.parent() {
                fs::create_dir_all(dir)?;
            }
            fs::write(out, bytes)?;
        }
        copied += 1;
    }
    println!(
        "extracted {copied} of {} assets into {}",
        manifest.asset.len(),
        vendor.display()
    );
    Ok(())
}

/// Stores the two images of an image-based Lottie: the layer drawn under a
/// track matte ("column") and the plain layer below it ("base").
fn split_lottie(bytes: &[u8], out: &Path) -> Result<()> {
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes))?;
    let mut json = None;
    let mut images = HashMap::new();
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i)?;
        let name = entry.name()?.to_string();
        let mut data = Vec::new();
        entry.read_to_end(&mut data)?;
        if name.ends_with(".json") {
            json = Some(data);
        } else if name.ends_with(".png") {
            let file = name.rsplit('/').next().unwrap_or(&name).to_string();
            images.insert(file, data);
        }
    }
    let json: serde_json::Value = serde_json::from_slice(&json.context("lottie without json")?)?;
    let assets: HashMap<String, String> = json["assets"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|a| Some((a["id"].as_str()?.to_string(), a["p"].as_str()?.to_string())))
        .collect();
    if let Some(dir) = out.parent() {
        fs::create_dir_all(dir)?;
    }
    for layer in json["layers"].as_array().into_iter().flatten() {
        let Some(asset) = layer["refId"].as_str().and_then(|r| assets.get(r)) else {
            continue;
        };
        let role = if layer["tt"].is_null() {
            "base"
        } else {
            "column"
        };
        if let Some(data) = images.get(asset) {
            let stem = out.file_name().and_then(|s| s.to_str()).unwrap_or("lottie");
            fs::write(out.with_file_name(format!("{stem}_{role}.png")), data)?;
        }
    }
    Ok(())
}

fn check() -> Result<()> {
    let manifest = manifest()?;
    let vendor = root().join("assets/vendor");
    let mut missing = 0;
    for asset in &manifest.asset {
        let ext = Path::new(&asset.path)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("bin");
        let present = if ext == "zip" {
            vendor.join(format!("{}_base.png", asset.key)).exists()
        } else {
            vendor.join(format!("{}.{ext}", asset.key)).exists()
        };
        if !present {
            println!("missing {}", asset.key);
            missing += 1;
        }
    }
    println!("{missing} missing of {}", manifest.asset.len());
    Ok(())
}

fn check_public() -> Result<()> {
    let output = Process::new("git")
        .args(["ls-files"])
        .current_dir(root())
        .output()?;
    let files = String::from_utf8_lossy(&output.stdout);
    let bad: Vec<&str> = files
        .lines()
        .filter(|f| {
            f.starts_with("assets/vendor/")
                || f.starts_with(".research/")
                || f.starts_with("captures/")
                || (f.ends_with(".xcap") && !f.contains("/fixtures/"))
        })
        .collect();
    if bad.is_empty() {
        println!("no private files are tracked");
        return Ok(());
    }
    for f in &bad {
        println!("tracked private file: {f}");
    }
    bail!("{} private files are tracked", bad.len())
}

fn bundle(release: bool) -> Result<()> {
    let root = root();
    let profile = if release { "release" } else { "debug" };
    let mut build = Process::new("cargo");
    build.args(["build", "-p", "xilo"]).current_dir(&root);
    if release {
        build.arg("--release");
    }
    if !build.status()?.success() {
        bail!("build failed");
    }
    let app = root.join("dist/Xilo.app");
    let contents = app.join("Contents");
    let _ = fs::remove_dir_all(&app);
    fs::create_dir_all(contents.join("MacOS"))?;
    fs::create_dir_all(contents.join("Resources"))?;
    fs::copy(
        root.join(format!("target/{profile}/xilo")),
        contents.join("MacOS/Xilo"),
    )?;
    let version = env!("CARGO_PKG_VERSION");
    fs::write(
        contents.join("Info.plist"),
        INFO_PLIST.replace("{version}", version),
    )?;
    let icon = root.join("assets/builtin/icons/Xilo.icns");
    if icon.exists() {
        fs::copy(&icon, contents.join("Resources/Xilo.icns"))?;
    }
    let vendor = root.join("assets/vendor");
    if vendor.exists() {
        copy_dir(&vendor, &contents.join("Resources/vendor"))?;
    }
    let identity = std::env::var("XILO_SIGN_IDENTITY").unwrap_or_else(|_| "-".into());
    let status = Process::new("codesign")
        .args(["--force", "--deep", "--sign", &identity])
        .arg(&app)
        .status()?;
    if !status.success() {
        bail!("codesign failed");
    }
    println!("built {}", app.display());
    Ok(())
}

fn copy_dir(from: &Path, to: &Path) -> Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

const INFO_PLIST: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key><string>Xilo</string>
    <key>CFBundleDisplayName</key><string>Xilo</string>
    <key>CFBundleIdentifier</key><string>dev.xilo.app</string>
    <key>CFBundleVersion</key><string>{version}</string>
    <key>CFBundleShortVersionString</key><string>{version}</string>
    <key>CFBundleExecutable</key><string>Xilo</string>
    <key>CFBundleIconFile</key><string>Xilo</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>LSMinimumSystemVersion</key><string>13.0</string>
    <key>NSHighResolutionCapable</key><true/>
    <key>NSBluetoothAlwaysUsageDescription</key><string>Xilo connects to your Anker SOLIX devices over Bluetooth.</string>
</dict>
</plist>
"#;
