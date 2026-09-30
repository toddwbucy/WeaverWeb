//! The upstream model adapter (Spec section 10). Not implemented in
//! the first scaffold. Its conversation router retired under W2;
//! implementing a provider remains a separate act.

use crate::config::ProviderConfig;

// The seam is declared ahead of its first implementation on purpose
// (Spec sections 10 and 15 of the retired Spec). The conversation
// router that named model participants retired under W2.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct UpstreamAdapter {
    pub provider: ProviderConfig,
}

#[allow(dead_code)]
impl UpstreamAdapter {
    pub fn new(provider: ProviderConfig) -> Self {
        Self { provider }
    }

    pub async fn turn(&self, _context: &str) -> anyhow::Result<String> {
        anyhow::bail!(
            "upstream provider '{}' is configured but the adapter is not yet implemented",
            self.provider.name
        )
    }
}
