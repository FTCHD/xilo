#[cfg(target_os = "macos")]
use std::path::PathBuf;

#[cfg(target_os = "macos")]
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

/// Adds or removes the desktop entry that starts Xilo when the user logs in.
#[cfg(target_os = "linux")]
pub fn set(enabled: bool) -> std::io::Result<()> {
    let dir = directories::BaseDirs::new()
        .map(|d| d.config_dir().join("autostart"))
        .ok_or_else(|| std::io::Error::other("no home folder"))?;
    let exe = std::env::current_exe()?;
    autostart::set(&dir, &exe.to_string_lossy(), enabled)
}

/// Adds or removes Xilo in the programs that Windows starts when the user logs in.
#[cfg(windows)]
pub fn set(enabled: bool) -> std::io::Result<()> {
    const NAME: &str = "Xilo";
    let run = windows_registry::CURRENT_USER
        .create(r"Software\Microsoft\Windows\CurrentVersion\Run")
        .map_err(std::io::Error::other)?;
    let result = if enabled {
        let exe = std::env::current_exe()?;
        run.set_string(NAME, format!("\"{}\"", exe.display()))
    } else if run.get_string(NAME).is_ok() {
        run.remove_value(NAME)
    } else {
        Ok(())
    };
    result.map_err(std::io::Error::other)
}

#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
pub fn set(_enabled: bool) -> std::io::Result<()> {
    Err(std::io::Error::other(
        "open at login is not supported on this system yet",
    ))
}

/// The XDG autostart entry that Linux desktops read at login.
#[cfg(any(target_os = "linux", test))]
mod autostart {
    use std::path::Path;

    const FILE: &str = "dev.xilo.Xilo.desktop";

    pub fn set(dir: &Path, exe: &str, enabled: bool) -> std::io::Result<()> {
        let file = dir.join(FILE);
        if !enabled {
            return match std::fs::remove_file(&file) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
                _ => Ok(()),
            };
        }
        std::fs::create_dir_all(dir)?;
        std::fs::write(file, entry(exe))
    }

    pub fn entry(exe: &str) -> String {
        format!(
            "[Desktop Entry]\nType=Application\nName=Xilo\nExec={}\nX-GNOME-Autostart-enabled=true\n",
            exec(exe)
        )
    }

    /// A path for the Exec key. In the quotes, `"`, `` ` ``, `$` and `\` take a backslash.
    /// Then the file format doubles every backslash, and `%` becomes `%%`.
    fn exec(path: &str) -> String {
        let mut quoted = String::from("\"");
        for c in path.chars() {
            if matches!(c, '"' | '`' | '$' | '\\') {
                quoted.push('\\');
            }
            quoted.push(c);
        }
        quoted.push('"');
        quoted.replace('\\', "\\\\").replace('%', "%%")
    }
}

/// The app bundle when running from one, else the executable.
#[cfg(target_os = "macos")]
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

#[cfg(target_os = "macos")]
fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(target_os = "macos")]
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
    #[cfg(target_os = "macos")]
    #[test]
    fn agent_escapes_paths() {
        let text = super::agent(&["/Apps/A&B.app".into()]);
        assert!(text.contains("<string>/Apps/A&amp;B.app</string>"));
        assert!(text.contains("<key>RunAtLoad</key>"));
    }

    #[test]
    fn autostart_entry_quotes_the_path() {
        let entry = super::autostart::entry("/opt/My Apps/50% \"x\"/xilo-app");
        assert!(
            entry.contains(r#"Exec="/opt/My Apps/50%% \\"x\\"/xilo-app""#),
            "{entry}"
        );
    }

    #[test]
    fn autostart_adds_and_removes_the_entry() {
        let dir = std::env::temp_dir().join(format!("xilo-autostart-{}", std::process::id()));
        let file = dir.join("dev.xilo.Xilo.desktop");
        super::autostart::set(&dir, "/usr/bin/xilo-app", true).expect("add");
        assert!(file.exists());
        super::autostart::set(&dir, "/usr/bin/xilo-app", false).expect("remove");
        assert!(!file.exists());
        super::autostart::set(&dir, "/usr/bin/xilo-app", false).expect("already gone");
        std::fs::remove_dir_all(&dir).expect("clean up");
    }
}
