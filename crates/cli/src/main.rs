use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use clap::{Parser, Subcommand};
use xilo_engine::sim::{SimConnector, SimDevice};
use xilo_engine::{
    Connector, DeviceAddr, DeviceHandle, DeviceSetup, Engine, EngineConfig, Scanner, Snapshot,
};
use xilo_models::{CommandRequest, Confirmed, Input, Registry};
use xilo_proto::{Cipher, Frame, MsgId, Request, TimestampMode, tlv};

#[derive(Parser)]
#[command(
    name = "xilo-cli",
    about = "Talk to Anker SOLIX devices over Bluetooth"
)]
struct Cli {
    /// Peripheral id from `scan`. Without it, the first SOLIX device found is used.
    #[arg(long, global = true)]
    device: Option<String>,
    /// Pick the device whose name contains this text.
    #[arg(long, global = true)]
    name: Option<String>,
    /// Model code, e.g. A1723. Detected automatically when left out.
    #[arg(long, global = true)]
    model: Option<String>,
    /// Use a simulated device of this model instead of Bluetooth.
    #[arg(long, global = true)]
    sim: Option<String>,
    /// Allow commands on models that are not verified on hardware.
    #[arg(long, global = true)]
    allow_unverified: bool,
    /// Use SolixBLE's fixed base timestamp instead of UTC.
    #[arg(long, global = true)]
    solix_timestamps: bool,
    /// Print diagnostic logs.
    #[arg(long, short, global = true)]
    verbose: bool,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// List SOLIX devices nearby.
    Scan {
        #[arg(long, default_value_t = 8)]
        secs: u64,
    },
    /// Connect and print the current state.
    Status {
        /// Also print unmapped raw keys.
        #[arg(long)]
        raw: bool,
    },
    /// Print values as they change.
    Watch {
        #[arg(long, default_value_t = 0)]
        secs: u64,
    },
    /// Send a command, e.g. `set dc_switch 1`.
    Set {
        command: String,
        value: Option<i64>,
        /// Confirm a dangerous command.
        #[arg(long)]
        confirm: bool,
    },
    /// Send a raw request, e.g. `raw 004b a2=u8:1`.
    Raw { msg: String, params: Vec<String> },
    /// List the commands of the device's model.
    Commands,
    /// List known models.
    Models,
    /// Decrypt and parse frames with a known shared secret.
    Decode {
        #[arg(long)]
        secret: String,
        frames: Vec<String>,
    },
    /// Print every frame sent and received.
    Trace {
        #[arg(long, default_value_t = 30)]
        secs: u64,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    if cli.verbose {
        tracing_subscriber::fmt()
            .with_env_filter("debug,btleplug=debug")
            .with_writer(std::io::stderr)
            .init();
    }
    let registry = Arc::new(Registry::builtin().clone());
    match &cli.command {
        Command::Models => {
            for m in registry.all() {
                let status = if m.is_verified() {
                    "verified"
                } else {
                    "unverified"
                };
                println!("{}  {:<22} {:<8} {}", m.code, m.name, m.family.id, status);
            }
            return Ok(());
        }
        Command::Decode { secret, frames } => return decode(secret, frames),
        _ => {}
    }

    let (connector, scanner): (Arc<dyn Connector>, Arc<dyn Scanner>) = match &cli.sim {
        Some(code) => {
            let sim = SimConnector::new();
            let spec = registry
                .get(code)
                .ok_or_else(|| anyhow!("unknown model {code}"))?
                .clone();
            sim.add("sim", SimDevice::new(spec, "ALFSBK1G00000001"));
            (Arc::new(sim.clone()), Arc::new(sim))
        }
        None => {
            let ble = xilo_ble::BleTransport::new().await?;
            (ble.clone(), ble)
        }
    };
    let engine = Engine::new(
        connector,
        scanner,
        registry.clone(),
        EngineConfig {
            client_id: xilo_engine::identity::new_client_id(),
            timestamps: if cli.solix_timestamps {
                TimestampMode::SessionBase(xilo_proto::zx::SOLIX_BASE_TIMESTAMP)
            } else {
                TimestampMode::Utc
            },
        },
    );

    if let Command::Scan { secs } = cli.command {
        return scan(&engine, secs).await;
    }

    let addr = resolve(&cli, &engine).await?;
    let handle = engine.add(DeviceSetup {
        key: addr.id.clone(),
        addr,
        name: None,
        model: cli.model.clone(),
        allow_unverified: cli.allow_unverified,
        auto_connect: true,
    });
    let traces = matches!(cli.command, Command::Trace { .. }).then(|| handle.traces());
    let snap = wait(&handle, Duration::from_secs(60), |s| {
        s.is_ready() && s.model.is_some() && s.updated.is_some()
    })
    .await
    .with_context(|| {
        let s = handle.snapshot();
        format!(
            "device did not become ready ({}){}",
            s.link.label(),
            s.last_error
                .as_ref()
                .map(|e| format!(": {e}"))
                .unwrap_or_default()
        )
    })?;

    let result = match cli.command {
        Command::Status { raw } => {
            print_state(&snap, raw);
            Ok(())
        }
        Command::Watch { secs } => watch(&handle, secs).await,
        Command::Set {
            command,
            value,
            confirm,
        } => set(&handle, &snap, command, value, confirm).await,
        Command::Raw { msg, params } => raw(&handle, &msg, &params).await,
        Command::Commands => {
            print_commands(&snap);
            Ok(())
        }
        Command::Trace { secs } => trace(traces, secs).await,
        Command::Scan { .. } | Command::Models | Command::Decode { .. } => Ok(()),
    };
    engine.shutdown().await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    result
}

async fn scan(engine: &Engine, secs: u64) -> Result<()> {
    println!("scanning for {secs} s...");
    let found = engine.scan(Duration::from_secs(secs)).await?;
    if found.is_empty() {
        println!("no SOLIX devices found");
    }
    for f in found {
        let a = &f.advert;
        println!(
            "{}  {:<28} rssi {:>4}",
            a.addr.as_ref().map(|d| d.id.as_str()).unwrap_or("?"),
            a.name.as_deref().unwrap_or("-"),
            a.rssi.map(|r| r.to_string()).unwrap_or_default()
        );
        for (uuid, data) in &a.service_data {
            println!("    service data {uuid}: {}", hex::encode(data));
        }
        for (id, data) in &a.manufacturer_data {
            println!("    manufacturer {id:#06x}: {}", hex::encode(data));
        }
    }
    Ok(())
}

async fn resolve(cli: &Cli, engine: &Engine) -> Result<DeviceAddr> {
    if cli.sim.is_some() {
        return Ok(DeviceAddr::new("sim"));
    }
    if let Some(id) = &cli.device {
        return Ok(DeviceAddr::new(id.clone()));
    }
    let found = engine.scan(Duration::from_secs(6)).await?;
    let pick = found.into_iter().find(|f| match &cli.name {
        Some(n) => f
            .advert
            .name
            .as_deref()
            .is_some_and(|x| x.to_lowercase().contains(&n.to_lowercase())),
        None => true,
    });
    let pick = pick.ok_or_else(|| anyhow!("no matching SOLIX device found"))?;
    println!(
        "using {} ({})",
        pick.advert.name.as_deref().unwrap_or("?"),
        pick.advert
            .addr
            .as_ref()
            .map(|a| a.id.as_str())
            .unwrap_or("?")
    );
    pick.advert
        .addr
        .ok_or_else(|| anyhow!("device has no address"))
}

async fn wait(
    handle: &DeviceHandle,
    timeout: Duration,
    what: impl Fn(&Snapshot) -> bool,
) -> Option<Arc<Snapshot>> {
    let mut rx = handle.watch();
    tokio::time::timeout(timeout, async {
        loop {
            let snap = rx.borrow_and_update().clone();
            if what(&snap) {
                return Some(snap);
            }
            if rx.changed().await.is_err() {
                return None;
            }
        }
    })
    .await
    .ok()
    .flatten()
}

fn print_state(snap: &Snapshot, raw: bool) {
    let Some(model) = &snap.model else {
        return;
    };
    println!(
        "{} ({}){}",
        model.name,
        model.code,
        if model.is_verified() {
            ""
        } else {
            "  [unverified: read-only]"
        }
    );
    if let Some(info) = &snap.info {
        println!(
            "module {} {}, mtu {}, capability {:#04x}, serial {}",
            info.module,
            info.module_firmware,
            info.mtu,
            info.capability,
            info.serial.as_deref().unwrap_or("-")
        );
    }
    let mut shown = std::collections::HashSet::new();
    for field in &model.fields {
        if !shown.insert(field.id.clone()) {
            continue;
        }
        if let Some(value) = snap.fields.get(&field.id) {
            let flag = if field.hidden { " (hidden)" } else { "" };
            println!(
                "  {:<32} {}{flag}",
                field.label,
                model.format(&field.id, value)
            );
        }
    }
    if raw {
        println!("raw messages:");
        for (id, payload) in &snap.raw {
            let items: Vec<String> = payload
                .items
                .iter()
                .map(|i| format!("{:02x}={}", i.key, hex::encode(i.raw())))
                .collect();
            println!("  {id}: {}", items.join(" "));
        }
    }
}

fn print_commands(snap: &Snapshot) {
    let Some(model) = &snap.model else {
        return;
    };
    for c in model.commands.values() {
        let current = model
            .current(c, &snap.fields)
            .map(|v| format!(" (now {v})"))
            .unwrap_or_default();
        let values = match c.input {
            Input::Switch => format!("{} on / {} off", c.on_value, c.off_value),
            Input::Choice => c
                .options
                .iter()
                .map(|o| format!("{}={}", o.value, o.label))
                .collect::<Vec<_>>()
                .join(", "),
            Input::Number => format!(
                "{}..{} step {}",
                c.min.unwrap_or(0),
                c.max.unwrap_or(0),
                c.step.unwrap_or(1)
            ),
            Input::None => "no value".into(),
        };
        let unsupported = if snap.unsupported.contains(&c.id) {
            " [no ack]"
        } else {
            ""
        };
        println!(
            "  {:<18} {:<40} {values}{current}{unsupported}",
            c.id, c.label
        );
    }
}

async fn watch(handle: &DeviceHandle, secs: u64) -> Result<()> {
    let mut rx = handle.watch();
    let mut last = rx.borrow().fields.clone();
    let run = async {
        while rx.changed().await.is_ok() {
            let snap = rx.borrow_and_update().clone();
            let Some(model) = &snap.model else { continue };
            for (id, value) in &snap.fields {
                if last.get(id) != Some(value) {
                    let label = model.field(id).map(|f| f.label.as_str()).unwrap_or(id);
                    println!("{label}: {}", model.format(id, value));
                }
            }
            last = snap.fields.clone();
        }
    };
    if secs == 0 {
        run.await;
    } else {
        let _ = tokio::time::timeout(Duration::from_secs(secs), run).await;
    }
    Ok(())
}

async fn set(
    handle: &DeviceHandle,
    snap: &Snapshot,
    command: String,
    value: Option<i64>,
    confirm: bool,
) -> Result<()> {
    let mut request = CommandRequest::new(command.clone(), value);
    if confirm {
        request = request.confirmed(Confirmed::by_user());
    }
    let outcome = handle.send(request).await?;
    println!("{command}: {outcome:?}");
    tokio::time::sleep(Duration::from_secs(3)).await;
    let after = handle.snapshot();
    if let Some(model) = &snap.model
        && let Some(state) = model.command(&command).and_then(|c| c.state.clone())
        && let Some(v) = after.fields.get(&state)
    {
        println!("{state} is now {}", model.format(&state, v));
    }
    Ok(())
}

async fn raw(handle: &DeviceHandle, msg: &str, params: &[String]) -> Result<()> {
    let id = MsgId::parse(msg).ok_or_else(|| anyhow!("bad message id {msg}"))?;
    let mut parsed = Vec::new();
    for p in params {
        let (key, rest) = p
            .split_once('=')
            .ok_or_else(|| anyhow!("use key=type:value"))?;
        let (ty, value) = rest
            .split_once(':')
            .ok_or_else(|| anyhow!("use key=type:value"))?;
        let key = u8::from_str_radix(key, 16)?;
        let value = match ty {
            "u8" => tlv::Value::U8(value.parse()?),
            "u16" => tlv::Value::U16(value.parse()?),
            "u32" => tlv::Value::U32(value.parse()?),
            "str" => tlv::Value::Str(value.into()),
            "hex" => tlv::Value::Bytes(hex::decode(value)?),
            other => bail!("unknown type {other}"),
        };
        parsed.push((key, value));
    }
    let outcome = handle.send_raw(Request { id, params: parsed }).await?;
    println!("{id}: {outcome:?}");
    Ok(())
}

async fn trace(
    traces: Option<tokio::sync::broadcast::Receiver<xilo_proto::Trace>>,
    secs: u64,
) -> Result<()> {
    let Some(mut rx) = traces else {
        return Ok(());
    };
    let _ = tokio::time::timeout(Duration::from_secs(secs), async {
        while let Ok(t) = rx.recv().await {
            let dir = if t.outgoing { "->" } else { "<-" };
            let cmd = t.cmd.map(|c| c.to_string()).unwrap_or_default();
            let plain = t.plain.map(hex::encode).unwrap_or_default();
            let note = t.note.unwrap_or_default();
            println!("{dir} {cmd:<5} {plain} {note}");
        }
    })
    .await;
    Ok(())
}

fn decode(secret: &str, frames: &[String]) -> Result<()> {
    let secret: [u8; 32] = hex::decode(secret)?
        .try_into()
        .map_err(|_| anyhow!("secret must be 32 bytes"))?;
    let cipher = Cipher::cbc_from_secret(&secret);
    for f in frames {
        let frame = Frame::decode(&hex::decode(f)?)?;
        let plain = if frame.cmd.is_encrypted() {
            cipher.open(&frame.body)?
        } else {
            frame.body.clone()
        };
        let payload = tlv::parse(&plain, frame.cmd.is_response(), frame.function != 0x01);
        println!("{} status {:?}", frame.cmd, payload.status);
        for item in payload.items {
            println!("  {:02x} {}", item.key, hex::encode(item.raw()));
        }
    }
    Ok(())
}
