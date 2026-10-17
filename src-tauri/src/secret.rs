//! OpenAI API key storage in the OS credential store.
//!
//! On Windows the key goes to the Credential Manager (DPAPI-encrypted, tied to
//! the user account) and on macOS to the Keychain, so it never sits in plain
//! text in `settings.json`. On other platforms there is no store wired in and
//! [`settings`](crate::settings) keeps it in the settings file instead.

use anyhow::Result;

#[cfg(any(windows, target_os = "macos"))]
mod imp {
    use anyhow::{anyhow, Result};
    use keyring::{Entry, Error};

    use crate::i18n::tf;

    const SERVICE: &str = "com.whisperaround.app";
    const ACCOUNT: &str = "openai_api_key";

    fn entry() -> Result<Entry> {
        Entry::new(SERVICE, ACCOUNT).map_err(|e| anyhow!(tf("error.keychain.open", &[("error", &e)])))
    }

    pub fn load() -> Result<Option<String>> {
        match entry()?.get_password() {
            Ok(key) => Ok(Some(key)),
            Err(Error::NoEntry) => Ok(None),
            Err(e) => Err(anyhow!(tf("error.keychain.read", &[("error", &e)]))),
        }
    }

    pub fn store(key: &str) -> Result<()> {
        let entry = entry()?;
        let result = if key.is_empty() {
            match entry.delete_credential() {
                Err(Error::NoEntry) => Ok(()),
                other => other,
            }
        } else {
            entry.set_password(key)
        };
        result.map_err(|e| anyhow!(tf("error.keychain.write", &[("error", &e)])))
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
mod imp {
    use anyhow::{anyhow, Result};

    use crate::i18n::t;

    pub fn load() -> Result<Option<String>> {
        Err(anyhow!(t("error.keychain.unavailable")))
    }

    pub fn store(_key: &str) -> Result<()> {
        Err(anyhow!(t("error.keychain.unavailable")))
    }
}

/// Whether this build stores the key in the OS credential store.
pub const AVAILABLE: bool = cfg!(any(windows, target_os = "macos"));

/// The stored key, `None` if there is none.
pub fn load() -> Result<Option<String>> {
    imp::load()
}

/// Store the key; an empty key removes it.
pub fn store(key: &str) -> Result<()> {
    imp::store(key)
}
