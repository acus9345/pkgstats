//! Connected accounts. The GitHub token is kept in the OS credential store
//! (Windows Credential Manager), never in PkgStats' own JSON files.

const SERVICE: &str = "PkgStats";
#[cfg(not(test))]
const GITHUB_ENTRY: &str = "github-token";
// Tests use their own entry so they never touch a real saved token.
#[cfg(test)]
const GITHUB_ENTRY: &str = "github-token-test";

fn github_entry() -> Result<keyring::Entry, String> {
    keyring::Entry::new(SERVICE, GITHUB_ENTRY).map_err(|e| e.to_string())
}

pub fn load_github_token() -> Option<String> {
    github_entry().ok()?.get_password().ok().filter(|t| !t.is_empty())
}

pub fn save_github_token(token: &str) -> Result<(), String> {
    github_entry()?.set_password(token).map_err(|e| e.to_string())
}

pub fn delete_github_token() -> Result<(), String> {
    match github_entry()?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Uses the real OS credential store: `cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn token_round_trips_through_credential_store() {
        save_github_token("test-value").unwrap();
        assert_eq!(load_github_token().as_deref(), Some("test-value"));
        delete_github_token().unwrap();
        assert_eq!(load_github_token(), None);
        delete_github_token().unwrap(); // deleting twice is fine
    }
}
