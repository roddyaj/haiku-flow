use serde::Serialize;
use std::{
    io::Read,
    process::{Command, Stdio},
    sync::{mpsc, Mutex, OnceLock},
    time::Duration,
};

const SERVICE: &str = "haiku-flow";
const ACCOUNT: &str = "gitlab-token";
const VAR: &str = "GITLAB_TOKEN";
const BEGIN: &str = "__HF_BEGIN__";
const END: &str = "__HF_END__";
const SHELL_TIMEOUT: Duration = Duration::from_secs(5);

/// Where the GitLab token was found.
#[derive(Clone, Copy, Debug, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum TokenSource {
    Env,
    Shell,
    Keychain,
    None,
}

/// Token lookup with the login-shell and keychain results cached.
pub struct Tokens {
    shell: OnceLock<Option<String>>,
    /// Outer `None` until the keychain has been read.
    keychain: Mutex<Option<Option<String>>>,
}

impl Tokens {
    pub fn new() -> Self {
        Self {
            shell: OnceLock::new(),
            keychain: Mutex::new(None),
        }
    }

    /// Resolve the token from the environment, then the login shell, then the keychain.
    /// Blocks while the login shell runs for the first time.
    pub fn resolve(&self) -> (Option<String>, TokenSource) {
        if let Some(t) = non_empty(std::env::var(VAR).ok()) {
            return (Some(t), TokenSource::Env);
        }
        if let Some(t) = self.shell.get_or_init(read_shell_token) {
            return (Some(t.clone()), TokenSource::Shell);
        }
        match self.keychain_token() {
            Some(t) => (Some(t), TokenSource::Keychain),
            None => (None, TokenSource::None),
        }
    }

    /// Run the login-shell lookup now, so later calls do not wait for it.
    pub fn warm_up(&self) {
        if non_empty(std::env::var(VAR).ok()).is_none() {
            self.shell.get_or_init(read_shell_token);
        }
    }

    /// Store the token in the keychain.
    pub fn save(&self, token: &str) -> Result<(), String> {
        entry()?.set_password(token).map_err(|e| e.to_string())?;
        *self.keychain.lock().unwrap() = Some(Some(token.to_string()));
        Ok(())
    }

    /// Remove the token from the keychain.
    pub fn clear(&self) -> Result<(), String> {
        match entry()?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => {}
            Err(e) => return Err(e.to_string()),
        }
        *self.keychain.lock().unwrap() = Some(None);
        Ok(())
    }

    /// Read the keychain once and cache the result.
    fn keychain_token(&self) -> Option<String> {
        let mut cached = self.keychain.lock().unwrap();
        cached
            .get_or_insert_with(|| entry().ok()?.get_password().ok().filter(|t| !t.is_empty()))
            .clone()
    }
}

/// Extract the token printed between the markers, ignoring shell startup noise.
pub fn parse_marked(out: &str) -> Option<String> {
    let start = out.find(BEGIN)? + BEGIN.len();
    let end = start + out[start..].find(END)?;
    non_empty(Some(out[start..end].trim().to_string()))
}

fn entry() -> Result<keyring::Entry, String> {
    keyring::Entry::new(SERVICE, ACCOUNT).map_err(|e| e.to_string())
}

fn non_empty(value: Option<String>) -> Option<String> {
    value.filter(|v| !v.trim().is_empty())
}

/// Ask the user's login shell for the token. GUI launches do not inherit
/// variables exported from shell startup files.
fn read_shell_token() -> Option<String> {
    let shell = non_empty(std::env::var("SHELL").ok()).unwrap_or_else(|| "/bin/zsh".to_string());
    let script = format!("printf '%s%s%s' '{BEGIN}' \"${VAR}\" '{END}'");
    let mut child = Command::new(shell)
        .args(["-lic", &script])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let mut stdout = child.stdout.take()?;
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut buf = String::new();
        let _ = stdout.read_to_string(&mut buf);
        let _ = tx.send(buf);
    });
    let result = rx.recv_timeout(SHELL_TIMEOUT).ok();
    if result.is_none() {
        let _ = child.kill();
    }
    let _ = child.wait();
    parse_marked(&result?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marked_token_ignores_surrounding_noise() {
        assert_eq!(
            parse_marked("compinit warning\n__HF_BEGIN__glpat-abc__HF_END__\n"),
            Some("glpat-abc".into())
        );
    }

    #[test]
    fn empty_or_unmarked_output_is_none() {
        assert_eq!(parse_marked("__HF_BEGIN____HF_END__"), None);
        assert_eq!(parse_marked("glpat-abc"), None);
        assert_eq!(parse_marked("__HF_BEGIN__glpat"), None);
    }
}
