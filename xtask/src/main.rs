use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{Read, Write};
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
    /// Build the release downloads for one target in dist/release.
    Dist {
        /// Rust target triple. The default is this computer.
        #[arg(long)]
        target: Option<String>,
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
        Command::Dist { target } => dist(target),
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
        let out = vendor.join(format!("{}.{ext}", asset.key));
        if let Some(dir) = out.parent() {
            fs::create_dir_all(dir)?;
        }
        fs::write(out, bytes)?;
        copied += 1;
    }
    println!(
        "extracted {copied} of {} assets into {}",
        manifest.asset.len(),
        vendor.display()
    );
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
        if !vendor.join(format!("{}.{ext}", asset.key)).exists() {
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
            f.starts_with(".research/")
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
    make_app(&app, &root.join(format!("target/{profile}")))?;
    println!("built {}", app.display());
    Ok(())
}

/// Writes a signed Xilo.app from the binaries in `bin`.
fn make_app(app: &Path, bin: &Path) -> Result<()> {
    let root = root();
    let contents = app.join("Contents");
    let _ = fs::remove_dir_all(app);
    fs::create_dir_all(contents.join("MacOS"))?;
    fs::create_dir_all(contents.join("Resources"))?;
    fs::copy(bin.join("xilo-app"), contents.join("MacOS/Xilo"))?;
    // Not in MacOS/, where "xilo" and "Xilo" would be the same file.
    if bin.join("xilo").exists() {
        fs::create_dir_all(contents.join("Helpers"))?;
        fs::copy(bin.join("xilo"), contents.join("Helpers/xilo"))?;
    }
    fs::write(
        contents.join("Info.plist"),
        INFO_PLIST.replace("{version}", env!("CARGO_PKG_VERSION")),
    )?;
    let icon = root.join("assets/builtin/icons/Xilo.icns");
    if icon.exists() {
        fs::copy(&icon, contents.join("Resources/Xilo.icns"))?;
    }
    let identity = std::env::var("XILO_SIGN_IDENTITY").unwrap_or_else(|_| "-".into());
    let status = Process::new("codesign")
        .args(["--force", "--deep", "--sign", &identity])
        .arg(app)
        .status()?;
    if !status.success() {
        bail!("codesign failed");
    }
    Ok(())
}

fn host_target() -> Result<String> {
    let out = Process::new("rustc").arg("-vV").output()?;
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .find_map(|l| l.strip_prefix("host: ").map(str::to_string))
        .context("rustc did not report its host target")
}

/// The system and processor names used in download file names.
fn platform(target: &str) -> Result<(&'static str, &'static str)> {
    let os = if target.contains("apple-darwin") {
        "macos"
    } else if target.contains("windows") {
        "windows"
    } else if target.contains("linux") {
        "linux"
    } else {
        bail!("no release format for {target}");
    };
    let arch = match target.split('-').next() {
        Some("aarch64") => "arm64",
        Some("x86_64") => "x64",
        _ => bail!("no release format for {target}"),
    };
    Ok((os, arch))
}

fn dist(target: Option<String>) -> Result<()> {
    let root = root();
    let target = match target {
        Some(t) => t,
        None => host_target()?,
    };
    let (os, arch) = platform(&target)?;
    let mut build = Process::new("cargo");
    build
        .args([
            "build",
            "--release",
            "--locked",
            "-p",
            "xilo",
            "-p",
            "xilo-cli",
        ])
        .args(["--target", &target])
        .current_dir(&root);
    if os == "macos" {
        build.env("MACOSX_DEPLOYMENT_TARGET", "13.0");
    }
    if !build.status()?.success() {
        bail!("build failed");
    }

    let bin = root.join("target").join(&target).join("release");
    let out = root.join("dist/release");
    let _ = fs::remove_dir_all(&out);
    fs::create_dir_all(&out)?;
    let name = format!("Xilo-{}-{os}-{arch}", env!("CARGO_PKG_VERSION"));
    let docs = ["README.md", "THIRD_PARTY_NOTICES.md"];
    let file = match os {
        "macos" => {
            let stage = out.join("stage");
            make_app(&stage.join("Xilo.app"), &bin)?;
            for doc in docs {
                fs::copy(root.join(doc), stage.join(doc))?;
            }
            applications_link(&stage)?;
            let dmg = out.join(format!("{name}.dmg"));
            let status = Process::new("hdiutil")
                .args(["create", "-volname", "Xilo", "-format", "UDZO", "-ov"])
                .arg("-srcfolder")
                .arg(&stage)
                .arg(&dmg)
                .status()?;
            fs::remove_dir_all(&stage)?;
            if !status.success() {
                bail!("hdiutil failed");
            }
            dmg
        }
        "windows" => {
            let zip_path = out.join(format!("{name}.zip"));
            let mut zip = zip::ZipWriter::new(File::create(&zip_path)?);
            let options = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated);
            let files = [
                (bin.join("xilo-app.exe"), "Xilo.exe"),
                (bin.join("xilo.exe"), "bin/xilo.exe"),
                (root.join(docs[0]), docs[0]),
                (root.join(docs[1]), docs[1]),
            ];
            for (path, entry) in files {
                zip.start_file(format!("{name}/{entry}"), options)?;
                zip.write_all(&fs::read(&path).with_context(|| path.display().to_string())?)?;
            }
            zip.finish()?;
            zip_path
        }
        _ => {
            let stage = out.join("stage");
            let folder = stage.join(&name);
            fs::create_dir_all(folder.join("bin"))?;
            fs::copy(bin.join("xilo-app"), folder.join("Xilo"))?;
            fs::copy(bin.join("xilo"), folder.join("bin/xilo"))?;
            for doc in docs {
                fs::copy(root.join(doc), folder.join(doc))?;
            }
            let tar = out.join(format!("{name}.tar.gz"));
            let status = Process::new("tar")
                .arg("-czf")
                .arg(&tar)
                .arg("-C")
                .arg(&stage)
                .arg(&name)
                .status()?;
            fs::remove_dir_all(&stage)?;
            if !status.success() {
                bail!("tar failed");
            }
            linux_packages(&root, &target, &out, &name)?;
            tar
        }
    };
    println!("built {}", file.display());
    Ok(())
}

/// The .deb and .rpm packages, with the settings in crates/app/Cargo.toml.
fn linux_packages(root: &Path, target: &str, out: &Path, name: &str) -> Result<()> {
    let deb = out.join(format!("{name}.deb"));
    let status = Process::new("cargo")
        .args(["deb", "-p", "xilo", "--no-build", "--no-strip"])
        .args(["--target", target, "--output"])
        .arg(&deb)
        .current_dir(root)
        .status()?;
    if !status.success() {
        bail!("cargo deb failed. It needs cargo-deb: cargo install cargo-deb");
    }
    let rpm = out.join(format!("{name}.rpm"));
    let status = Process::new("cargo")
        .args([
            "generate-rpm",
            "-p",
            "crates/app",
            "--payload-compress",
            "xz",
        ])
        .args(["--target", target, "--output"])
        .arg(&rpm)
        .current_dir(root)
        .status()?;
    if !status.success() {
        bail!(
            "cargo generate-rpm failed. It needs cargo-generate-rpm: cargo install cargo-generate-rpm"
        );
    }
    println!("built {}", deb.display());
    println!("built {}", rpm.display());
    Ok(())
}

/// The shortcut to /Applications in the disk image, for drag and drop installs.
#[cfg(unix)]
fn applications_link(stage: &Path) -> Result<()> {
    std::os::unix::fs::symlink("/Applications", stage.join("Applications"))?;
    Ok(())
}

#[cfg(not(unix))]
fn applications_link(_stage: &Path) -> Result<()> {
    bail!("macOS disk images can only be built on macOS")
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
