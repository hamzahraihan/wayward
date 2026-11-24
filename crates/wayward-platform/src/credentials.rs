//! Windows Credential Manager access behind a safe, Win32-free interface.
//!
//! Secrets never enter `wayward.toml` and are never logged. The account key is
//! derived from the provider id; callers only ever see `Option<String>` or
//! opaque error strings that never contain the secret itself.

use keyring::{Entry, Error as KeyringError};

/// Service name under which wayward stores its credentials.
pub const SERVICE: &str = "wayward";

/// Build the credential account key for a provider id. Pure.
///
/// Provider ids are restricted to a small safe alphabet so a config value can
/// never address an unrelated credential in the Windows store.
pub fn account_for(provider_id: &str) -> Result<String, String> {
    if provider_id.is_empty() || provider_id.len() > 64 {
        return Err("provider id must be 1..=64 characters".to_owned());
    }
    if !provider_id
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err(format!(
            "provider id {provider_id:?} has invalid characters; use letters, digits, '-' or '_'"
        ));
    }
    Ok(format!("{SERVICE}:{provider_id}"))
}

fn entry(provider_id: &str) -> Result<Entry, String> {
    let account = account_for(provider_id)?;
    Entry::new(SERVICE, &account).map_err(|error| format!("credential store unavailable: {error}"))
}

/// Store a secret for a provider, replacing any previous value.
pub fn store(provider_id: &str, secret: &str) -> Result<(), String> {
    if secret.is_empty() {
        return Err("secret must not be empty".to_owned());
    }
    entry(provider_id)?
        .set_password(secret)
        .map_err(|error| format!("storing credential failed: {error}"))
}

/// Load a provider's secret. `Ok(None)` means no credential is stored.
pub fn load(provider_id: &str) -> Result<Option<String>, String> {
    match entry(provider_id)?.get_password() {
        Ok(secret) => Ok(Some(secret)),
        Err(KeyringError::NoEntry) => Ok(None),
        Err(error) => Err(format!("reading credential failed: {error}")),
    }
}

/// Remove a provider's credential. Returns `false` when none existed.
pub fn delete(provider_id: &str) -> Result<bool, String> {
    match entry(provider_id)?.delete_credential() {
        Ok(()) => Ok(true),
        Err(KeyringError::NoEntry) => Ok(false),
        Err(error) => Err(format!("deleting credential failed: {error}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn account_is_namespaced_by_service() {
        assert_eq!(
            account_for("claude-code").as_deref(),
            Ok("wayward:claude-code")
        );
    }

    #[test]
    fn account_allows_underscore() {
        assert_eq!(
            account_for("official_api").as_deref(),
            Ok("wayward:official_api")
        );
    }

    #[test]
    fn empty_provider_id_is_rejected() {
        assert!(account_for("").is_err());
    }

    #[test]
    fn overlong_provider_id_is_rejected() {
        assert!(account_for(&"a".repeat(65)).is_err());
    }

    #[test]
    fn provider_id_cannot_contain_separator_or_path() {
        for id in ["a:b", "a/b", "a\\b", "a b", "a\nb"] {
            assert!(account_for(id).is_err(), "expected rejection for {id:?}");
        }
    }

    #[test]
    fn empty_secret_is_rejected_before_store_access() {
        assert!(store("claude-code", "").is_err());
    }
}
