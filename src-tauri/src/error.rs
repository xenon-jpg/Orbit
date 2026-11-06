use serde::Serialize;

/// Errors crossing IPC must never contain cookies, launch tickets or response bodies.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("The vault is locked. Unlock it to continue.")]
    Locked,
    #[error("The vault is already unlocked.")]
    AlreadyUnlocked,
    #[error("Incorrect password or recovery code.")]
    BadPassword,
    #[error("The vault file is damaged or uses an unsupported version.")]
    BadVault,
    #[error("Windows account protection is unavailable on this platform.")]
    DpapiUnavailable,
    #[error("Windows Hello verification was not completed.")]
    HelloRequired,
    #[error("This account is not in your vault.")]
    NotFound,
    #[error("Invalid input: {0}")]
    Invalid(&'static str),
    #[error("Roblox sign-in expired. Add this account again.")]
    SessionExpired,
    #[error("Roblox requires browser verification or hardware-bound authentication. AltDeck will not bypass it.")]
    BrowserVerification,
    #[error("Roblox is rate limiting requests. Try again later.")]
    RateLimited,
    #[error("Roblox could not be reached. Check your connection.")]
    Network,
    #[error("Roblox returned an unsupported response (HTTP {0}).")]
    HttpStatus(u16),
    #[error("The operation needs the Windows desktop application.")]
    WindowsRequired,
    #[error("Roblox is not installed in the current user's standard location.")]
    NotInstalled,
    #[error("Windows verified the signature, but the Roblox signer could not be confirmed. Launch refused.")]
    UntrustedClient,
    #[error("Windows could not verify the Roblox executable signature (WinTrust code 0x{0:08X}). Launch refused.")]
    SignatureStatus(u32),
    #[error("Close existing Roblox windows before enabling multi-instance.")]
    ExistingClients,
    #[error("No suitable server has enough space for the whole batch.")]
    NoServer,
    #[error("The client is no longer managed by AltDeck.")]
    ClientGone,
    #[error("The launch was cancelled.")]
    Cancelled,
    #[error("Roblox launch failed at stage: {0}. No credentials were logged.")]
    LaunchStage(&'static str),
    #[error("Roblox launch failed at stage: {0} (OS code 0x{1:08X}). No credentials were logged.")]
    LaunchSystem(&'static str, u32),
    #[error("The operation could not be completed. See diagnostics for a redacted error code.")]
    Internal,
}

impl Serialize for Error {
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}
impl From<std::io::Error> for Error {
    fn from(_: std::io::Error) -> Self {
        Self::Internal
    }
}
impl From<serde_json::Error> for Error {
    fn from(_: serde_json::Error) -> Self {
        Self::BadVault
    }
}
impl From<reqwest::Error> for Error {
    fn from(_: reqwest::Error) -> Self {
        Self::Network
    }
}
pub type Result<T> = std::result::Result<T, Error>;
