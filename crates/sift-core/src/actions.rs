//! Running cleanup commands and opening Settings pages.

use std::process::Command;

fn base(program: &str) -> Command {
    // npm, yarn, pnpm are .cmd shims on Windows, which Command can't start
    // directly; going through cmd resolves them like a terminal would.
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let mut c = Command::new("cmd");
        c.arg("/C").arg(program).creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        c
    }
    #[cfg(not(windows))]
    {
        Command::new(program)
    }
}

/// Run a program to completion. Returns (exit code, combined output, trimmed).
pub fn run_command(program: &str, args: &[String]) -> (Option<i32>, String) {
    match base(program).args(args).output() {
        Ok(o) => {
            let mut text = String::from_utf8_lossy(&o.stdout).into_owned();
            let err = String::from_utf8_lossy(&o.stderr);
            if !err.trim().is_empty() {
                text.push('\n');
                text.push_str(&err);
            }
            let text = text.trim().to_string();
            // keep the tail; cache commands can be chatty
            let text = if text.len() > 4000 {
                let start = (text.len() - 4000..text.len()).find(|i| text.is_char_boundary(*i)).unwrap_or(0);
                format!("…{}", &text[start..])
            } else {
                text
            };
            (o.status.code(), text)
        }
        Err(e) => (None, format!("Couldn't start {program}: {e}")),
    }
}

/// Open a Windows Settings page such as `ms-settings:storagesense`.
pub fn open_settings(uri: &str) -> Result<(), String> {
    if !uri.starts_with("ms-settings:") {
        return Err("Not a Settings link".into());
    }
    #[cfg(windows)]
    {
        Command::new("explorer").arg(uri).spawn().map(|_| ()).map_err(|e| e.to_string())
    }
    #[cfg(not(windows))]
    {
        Err("Settings links only work on Windows".into())
    }
}
