//! Writes the website's device pages from the model definitions: one page for each
//! model, the devices index and the product pictures.

use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Result, bail};
use xilo_models::{CommandDef, Danger, Group, Input, ModelSpec, Protocol, Registry};

const HEADER: &str =
    "{/* Made by `cargo xtask site` from the model definitions. Changes here are overwritten. */}";

/// The groups on the devices page, in order.
const KINDS: [&str; 5] = [
    "Power stations",
    "Solarbanks",
    "Home",
    "Chargers and power banks",
    "Coolers",
];

fn kind(model: &ModelSpec) -> &'static str {
    let name = model.name.as_str();
    if name.contains("Solarbank") {
        "Solarbanks"
    } else if name.contains("Power Panel") {
        "Home"
    } else if name.contains("EverFrost") {
        "Coolers"
    } else if name.contains("Prime") || name.contains("MagGo") {
        "Chargers and power banks"
    } else {
        "Power stations"
    }
}

/// "SOLIX C200(X)" becomes "c200x", "Solarbank 2 Pro" becomes "solarbank-2-pro".
pub fn slug(model: &ModelSpec) -> String {
    let name = model.name.trim_start_matches("SOLIX ").to_lowercase();
    let mut out = String::new();
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if (c == ' ' || c == '-') && !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_matches('-').to_string()
}

/// "SOLIX C300X" becomes "Anker SOLIX C300X", "Solarbank 2 Pro" becomes "Anker SOLIX Solarbank 2 Pro".
fn full_name(model: &ModelSpec) -> String {
    let name = &model.name;
    if name.starts_with("SOLIX") || name.contains("Prime") || name.contains("MagGo") {
        format!("Anker {name}")
    } else {
        format!("Anker SOLIX {name}")
    }
}

/// A YAML string for the front matter, quoted so that colons are text.
fn yaml(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

/// Text that MDX reads as text, not as markup.
fn text(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('{', "\\{")
        .replace('}', "\\}")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('|', "\\|")
}

fn product_image(root: &Path, model: &ModelSpec) -> Option<PathBuf> {
    ["webp", "png"]
        .iter()
        .map(|ext| root.join(format!("assets/vendor/{}/product.{ext}", model.assets)))
        .find(|p| p.exists())
}

fn image_name(model: &ModelSpec, src: &Path) -> String {
    let ext = src.extension().and_then(|e| e.to_str()).unwrap_or("png");
    format!("{}.{ext}", slug(model))
}

/// What a command can be set to, in words.
fn values(command: &CommandDef) -> String {
    let unit = command.unit.as_deref().unwrap_or("");
    match command.input {
        Input::Switch => "On or off".into(),
        Input::Choice if !command.options.is_empty() => command
            .options
            .iter()
            .map(|o| text(&o.label))
            .collect::<Vec<_>>()
            .join(", "),
        Input::Number => match (command.min, command.max) {
            (Some(min), Some(max)) => {
                let step = command
                    .step
                    .filter(|s| *s > 1)
                    .map(|s| format!(", in steps of {s} {unit}"))
                    .unwrap_or_default();
                format!("{min} to {max} {unit}{step}").trim().to_string()
            }
            _ => "A number".into(),
        },
        Input::None => "An action".into(),
        Input::Choice => "A choice".into(),
    }
}

fn page(root: &Path, model: &ModelSpec) -> String {
    let name = full_name(model);
    let short = model.name.trim_start_matches("SOLIX ");
    let code = &model.code;
    let mut out = String::new();
    let description = format!(
        "Monitor and control the {name} ({code}) from macOS, Windows or Linux over Bluetooth, without an account: \
         battery, power, settings and automations."
    );
    out += &format!(
        "---\ntitle: {}\ndescription: {}\n---\n\n{HEADER}\n\n",
        yaml(&format!("{name} ({code})")),
        yaml(&description)
    );
    if let Some(src) = product_image(root, model) {
        let image = image_name(model, &src);
        out += &format!(
            "<DeviceImage src=\"/devices/{image}\" alt=\"{}\" />\n\n",
            text(&name)
        );
    }

    let dashboard = match model.family.dashboard.as_str() {
        "c300" | "c1000" => format!(
            "Xilo has a dashboard made for the {short}, like the one in the Anker app: the battery, \
             every port with its power, the AC and car outputs with live charts, and the light and screen controls."
        ),
        _ => format!(
            "Xilo shows the {short} on its general dashboard: the power in and out, the controls, \
             and every value the device reports."
        ),
    };
    out += &format!(
        "Xilo connects to the {name} ({code}) over Bluetooth, with no account, cloud or internet. {dashboard}\n\n"
    );

    let mut seen = HashSet::new();
    let fields: Vec<String> = model
        .fields
        .iter()
        .filter(|f| !f.hidden && seen.insert(f.label.clone()))
        .map(|f| match &f.unit {
            Some(unit) if !unit.is_empty() => format!("- {} ({})", text(&f.label), text(unit)),
            _ => format!("- {}", text(&f.label)),
        })
        .collect();
    if !fields.is_empty() {
        out += "## What Xilo shows\n\n<Columns>\n\n";
        out += &fields.join("\n");
        out += "\n\n</Columns>\n\n";
    }

    let mut groups: BTreeMap<u8, Vec<&CommandDef>> = BTreeMap::new();
    for c in model.commands.values() {
        let order = match c.group {
            Group::Control => 0,
            Group::Function => 1,
            Group::General => 2,
            Group::Hidden => continue,
        };
        groups.entry(order).or_default().push(c);
    }
    if !groups.is_empty() {
        out += "## What you can change\n\n";
        for (order, commands) in &groups {
            let title = ["Controls", "Settings", "General"][*order as usize];
            out += &format!("### {title}\n\n| | Values |\n| --- | --- |\n");
            for c in commands {
                let note = match c.danger {
                    Danger::None => "",
                    Danger::Confirm => " (asks first)",
                    Danger::Destructive => " (asks first, can't be undone)",
                };
                out += &format!("| {} | {}{note} |\n", text(&c.label), values(c));
            }
            out += "\n";
        }
        out += "Xilo checks every value against these limits before it sends anything, because the device itself accepts values it can't handle.\n\n";
    }

    let controls: Vec<String> = model
        .commands
        .values()
        .filter(|c| c.group == Group::Control && c.danger == Danger::None)
        .filter(|c| c.input == Input::Switch || (c.input == Input::Choice && !c.options.is_empty()))
        .map(|c| text(&c.label))
        .collect();
    let usb_wake = model.command("dc_switch").is_some()
        && model
            .fields
            .iter()
            .any(|f| f.id.starts_with("usb_") && f.id.ends_with("_power"));
    out += "## Automations\n\n";
    if controls.is_empty() {
        out += "This model has no controls that a schedule can switch. Alerts still work: errors, battery levels, AC power and connection.\n\n";
    } else {
        out += &format!(
            "- **Schedules** can switch {} at set times, on the days you choose.\n",
            controls.join(", ")
        );
        out += "- **Battery reserve** turns outputs off when the battery drops to a level you choose.\n";
        if usb_wake {
            out += "- **USB wake** keeps the USB ports on, for devices that turn them off after a while at low load.\n";
        }
        out += "- **Alerts** for errors, battery levels, AC power and connection.\n\n";
    }
    out += "See [Automations](/docs/automations) for how they work.\n\n";

    let protocol = match model.family.protocol {
        Protocol::Zx => "Anker's ZX Bluetooth protocol, encrypted",
        Protocol::Prime => "Anker's Prime Bluetooth protocol, encrypted",
    };
    out += &format!(
        "## Connection\n\nXilo talks to the {short} with {protocol}. It finds the device by itself when you scan, \
         and remembers it. The model number is {code}.\n\n"
    );
    out += "[Download Xilo](/docs/install) for macOS, Windows or Linux.\n";
    out
}

fn index(models: &[&ModelSpec]) -> String {
    let mut out = format!(
        "---\ntitle: Supported devices\ndescription: {}\n---\n\n{HEADER}\n\n",
        yaml(&format!(
            "The {} Anker SOLIX power stations, Solarbanks, chargers and coolers that Xilo can monitor and control over Bluetooth.",
            models.len()
        ))
    );
    out += &format!(
        "Xilo works with {} Anker models. Each page lists what Xilo shows and what you can change.\n\n",
        models.len()
    );
    for kind_name in KINDS {
        let group: Vec<&&ModelSpec> = models.iter().filter(|m| kind(m) == kind_name).collect();
        if group.is_empty() {
            continue;
        }
        out += &format!(
            "## {kind_name}\n\n| Model | Model number | Dashboard |\n| --- | --- | --- |\n"
        );
        for m in group {
            let dashboard = match m.family.dashboard.as_str() {
                "c300" | "c1000" => "Made for this model",
                _ => "General",
            };
            out += &format!(
                "| [{}](/docs/devices/{}) | {} | {dashboard} |\n",
                text(&full_name(m)),
                slug(m),
                m.code
            );
        }
        out += "\n";
    }
    out
}

fn meta(models: &[&ModelSpec]) -> String {
    let mut pages = vec!["\"index\"".to_string()];
    for kind_name in KINDS {
        let group: Vec<&&ModelSpec> = models.iter().filter(|m| kind(m) == kind_name).collect();
        if group.is_empty() {
            continue;
        }
        pages.push(format!("\"---{kind_name}---\""));
        pages.extend(group.iter().map(|m| format!("\"{}\"", slug(m))));
    }
    format!(
        "{{\n  \"title\": \"Devices\",\n  \"pages\": [\n    {}\n  ]\n}}\n",
        pages.join(",\n    ")
    )
}

/// Writes the pages and pictures. With `check`, only fails if they are out of date.
pub fn run(root: &Path, check: bool) -> Result<()> {
    let registry = Registry::builtin();
    let mut models: Vec<&ModelSpec> = registry.all().map(|m| m.as_ref()).collect();
    models.sort_by_key(|m| (KINDS.iter().position(|k| *k == kind(m)), m.name.clone()));

    let content = root.join("website/content/docs/devices");
    let public = root.join("website/public/devices");
    let mut files: Vec<(PathBuf, Vec<u8>)> = vec![
        (content.join("index.mdx"), index(&models).into_bytes()),
        (content.join("meta.json"), meta(&models).into_bytes()),
    ];
    for m in &models {
        files.push((
            content.join(format!("{}.mdx", slug(m))),
            page(root, m).into_bytes(),
        ));
        if let Some(src) = product_image(root, m) {
            files.push((public.join(image_name(m, &src)), fs::read(&src)?));
        }
    }

    let wanted: HashSet<PathBuf> = files.iter().map(|(p, _)| p.clone()).collect();
    let stale: Vec<PathBuf> = [&content, &public]
        .iter()
        .filter_map(|dir| fs::read_dir(dir).ok())
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| !wanted.contains(p))
        .collect();

    if check {
        let changed: Vec<&PathBuf> = files
            .iter()
            .filter(|(p, bytes)| fs::read(p).ok().as_deref() != Some(bytes.as_slice()))
            .map(|(p, _)| p)
            .chain(stale.iter())
            .collect();
        if changed.is_empty() {
            println!("the device pages are up to date");
            return Ok(());
        }
        for p in &changed {
            println!(
                "out of date: {}",
                p.strip_prefix(root).unwrap_or(p).display()
            );
        }
        bail!("run cargo xtask site to update the device pages");
    }

    fs::create_dir_all(&content)?;
    fs::create_dir_all(&public)?;
    for p in &stale {
        fs::remove_file(p)?;
    }
    for (p, bytes) in &files {
        fs::write(p, bytes)?;
    }
    println!("wrote {} device pages", models.len());
    Ok(())
}
