//! `install` / `uninstall` subcommands: print a host script to pipe into `sh`.
//! A container can't reach the host display or sound on its own, so the image ships the script that
//! sets up the right `docker run` flags, the login autostart and the hotkey.

const INSTALL: &str = include_str!("../scripts/install.sh");
const UNINSTALL: &str = include_str!("../scripts/uninstall.sh");
const RUN: &str = include_str!("../bin/countdown-run");
const TOGGLE: &str = include_str!("../bin/countdown-toggle");

/// Image reference baked in at build time (`IMAGE_REF` build arg).
fn image() -> String {
    std::env::var("COUNTDOWN_IMAGE_REF").unwrap_or_else(|_| "pythonking11/countdown:latest".into())
}

pub fn install_script() -> String {
    INSTALL.replace("@RUN_SCRIPT@", RUN).replace("@TOGGLE_SCRIPT@", TOGGLE).replace("@DEFAULT_IMAGE@", &image())
}

pub fn uninstall_script() -> String {
    UNINSTALL.replace("@DEFAULT_IMAGE@", &image())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scripts_are_complete_and_parse() {
        for (name, s) in [("install", install_script()), ("uninstall", uninstall_script())] {
            assert!(!s.contains("@DEFAULT_IMAGE@") && !s.contains("@RUN_SCRIPT@") && !s.contains("@TOGGLE_SCRIPT@"), "{name}");
            let out = std::process::Command::new("sh")
                .arg("-n")
                .stdin(std::process::Stdio::piped())
                .spawn()
                .and_then(|mut c| {
                    use std::io::Write;
                    c.stdin.take().unwrap().write_all(s.as_bytes())?;
                    c.wait_with_output()
                })
                .unwrap();
            assert!(out.status.success(), "{name} script has a syntax error");
        }
    }

    #[test]
    fn embedded_scripts_cannot_end_the_heredoc_early() {
        assert!(!RUN.contains("COUNTDOWN_EOF") && !TOGGLE.contains("COUNTDOWN_EOF"));
        assert!(RUN.ends_with('\n') && TOGGLE.ends_with('\n'));
        // The launcher placeholder survives into the install script, which fills it with the chosen image.
        assert!(install_script().contains("image=\"${COUNTDOWN_IMAGE:-@IMAGE@}\""));
    }
}
