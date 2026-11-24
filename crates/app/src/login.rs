use std::path::PathBuf;

const LABEL: &str = "dev.xilo.app";

/// Adds or removes Xilo from the programs that open at login.
#[cfg(target_os = "macos")]
pub fn set(enabled: bool) -> std::io::Result<()> {
    let agents = directories::BaseDirs::new()
        .map(|d| d.home_dir().join("Library/LaunchAgents"))
        .ok_or_else(|| std::io::Error::other("no home folder"))?;
    let plist = agents.join(format!("{LABEL}.plist"));
    if !enabled {
        return match std::fs::remove_file(&plist) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        };
    }
    let program = launch_target()?;
    std::fs::create_dir_all(&agents)?;
    std::fs::write(&plist, agent(&program))
}

#[cfg(not(target_os = "macos"))]
pub fn set(_enabled: bool) -> std::io::Result<()> {
    Err(std::io::Error::other(
        "open at login is not supported on this system yet",
    ))
}

/// The app bundle when running from one, else the executable.
fn launch_target() -> std::io::Result<Vec<String>> {
    let exe: PathBuf = std::env::current_exe()?;
    let bundle = exe
        .ancestors()
        .find(|p| p.extension().is_some_and(|e| e == "app"))
        .map(|p| p.to_path_buf());
    Ok(match bundle {
        Some(app) => vec![
            "/usr/bin/open".into(),
            "-a".into(),
            app.display().to_string(),
        ],
        None => vec![exe.display().to_string()],
    })
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn agent(program: &[String]) -> String {
    let args: String = program
        .iter()
        .map(|a| format!("        <string>{}</string>\n", escape(a)))
        .collect();
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{LABEL}</string>
    <key>ProgramArguments</key>
    <array>
{args}    </array>
    <key>RunAtLoad</key>
    <true/>
</dict>
</plist>
"#
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn agent_escapes_paths() {
        let text = super::agent(&["/Apps/A&B.app".into()]);
        assert!(text.contains("<string>/Apps/A&amp;B.app</string>"));
        assert!(text.contains("<key>RunAtLoad</key>"));
    }
}
