
use abis::ECDSAOwnedDKIMRegistry;
use anyhow::anyhow;
use chain::ChainClient;
use ethers::types::Address;
use ethers::utils::hex;
use relayer_utils::fr_to_bytes32;
use relayer_utils::public_key_hash;
use relayer_utils::ParsedEmail;
use relayer_utils::LOG;

use crate::*;
use candid::CandidType;
use ethers::types::Bytes;
use ethers::utils::hex::FromHex;
use ic_agent::agent::http_transport::ReqwestTransport;
use ic_agent::agent::*;
use ic_agent::identity::*;
use ic_utils::canister::*;

use serde::Deserialize;

/// Represents a client for interacting with the DKIM Oracle.
#[derive(Debug, Clone)]
pub struct DkimOracleClient<'a> {
    /// The canister used for communication
    pub canister: Canister<'a>,
}

/// Represents a signed DKIM public key.
#[derive(Default, CandidType, Deserialize, Debug, Clone)]
pub struct SignedDkimPublicKey {
    /// The selector for the DKIM key
    pub selector: String,
    /// The domain for the DKIM key
    pub domain: String,
    /// The signature of the DKIM key
    pub signature: String,
    /// The public key
    pub public_key: String,
    /// The hash of the public key
    pub public_key_hash: String,
}

impl<'a> DkimOracleClient<'a> {
    /// Generates an agent for the DKIM Oracle Client.
    ///
    /// # Arguments
    ///
    /// * `pem_path` - The path to the PEM file.
    /// * `replica_url` - The URL of the replica.
    ///
    /// # Returns
    ///
    /// An `anyhow::Result<Agent>`.
    pub fn gen_agent(pem_path: &str, replica_url: &str) -> anyhow::Result<Agent> {
        // Create identity from PEM file
        let identity = Secp256k1Identity::from_pem_file(pem_path)?;

        // Create transport using the replica URL
        let transport = ReqwestTransport::create(replica_url)?;

        // Build and return the agent
        let agent = AgentBuilder::default()
            .with_identity(identity)
            .with_transport(transport)
            .build()?;
        Ok(agent)
    }

    /// Creates a new DkimOracleClient.
    ///
    /// # Arguments
    ///
    /// * `canister_id` - The ID of the canister.
    /// * `agent` - The agent to use for communication.
    ///
    /// # Returns
    ///
    /// An `anyhow::Result<Self>`.
    pub fn new(canister_id: &str, agent: &'a Agent) -> anyhow::Result<Self> {
        // Build the canister using the provided ID and agent
        let canister = CanisterBuilder::new()
            .with_canister_id(canister_id)
            .with_agent(agent)
            .build()?;
        Ok(Self { canister })
    }

    /// Requests a signature for a DKIM public key.
    ///
    /// # Arguments
    ///
    /// * `selector` - The selector for the DKIM key.
    /// * `domain` - The domain for the DKIM key.
    ///
    /// # Returns
    ///
    /// An `anyhow::Result<SignedDkimPublicKey>`.
    pub async fn request_signature(
        &self,
        selector: &str,
        domain: &str,
    ) -> anyhow::Result<SignedDkimPublicKey> {
        // Build the request to sign the DKIM public key
        let request = self
            .canister
            .update("sign_dkim_public_key")
            .with_args((selector, domain))
            .build::<(Result<SignedDkimPublicKey, String>,)>();

        // Call the canister and wait for the response
        let response = request
            .call_and_wait_one::<Result<SignedDkimPublicKey, String>>()
            .await?
            .map_err(|e| anyhow!(format!("Error from canister: {:?}", e)))?;

        Ok(response)
    }
}

/// Checks and updates the DKIM for a given email.
///
/// # Arguments
///
/// * `email` - The email address.
/// * `parsed_email` - The parsed email data.
/// * `controller_eth_addr` - The Ethereum address of the controller.
/// * `wallet_addr` - The address of the wallet.
/// * `account_salt` - The salt for the account.
///
/// # Returns
///
/// A `Result<()>`.
pub async fn check_and_update_dkim(
    parsed_email: &ParsedEmail,
    dkim: Address,
    chain_client: ChainClient,
    relayer_state: RelayerState,
) -> Result<()> {
    return Ok(());
    // Generate public key hash
    let mut public_key_n = parsed_email.public_key.clone();
    public_key_n.reverse();
    let public_key_hash = public_key_hash(&public_key_n)?;
    info!(LOG, "public_key_hash {:?}", public_key_hash);

    // Get email domain
    let domain = parsed_email.get_email_domain()?;
    info!(LOG, "domain {:?}", domain);

    // Get DKIM
    let dkim = ECDSAOwnedDKIMRegistry::new(dkim, chain_client.client.clone());

    info!(LOG, "dkim {:?}", dkim);

    // Check if DKIM public key hash is valid
    if chain_client
        .check_if_dkim_public_key_hash_valid(
            domain.clone(),
            fr_to_bytes32(&public_key_hash)?,
            dkim.clone(),
        )
        .await?
    {
        info!(LOG, "public key registered");
        return Ok(());
    }

    // Get selector using regex
    let regex_pattern = r"((\r\n)|^)dkim-signature:([a-z]+=[^;]+; )+s=([0-9a-z_-]+);";
    let re = regex::Regex::new(regex_pattern).map_err(|e| anyhow!("Invalid regex: {}", e))?;

    let selector = re
        .captures(&parsed_email.canonicalized_header)
        .and_then(|caps| caps.get(4))
        .map(|m| m.as_str().to_string())
        .ok_or_else(|| anyhow!("Failed to extract selector using regex"))?;

    info!(LOG, "selector {}", selector);

    // Generate IC agent and create oracle client
    let ic_agent = DkimOracleClient::gen_agent(
        &relayer_state.config.path.pem,
        &relayer_state.config.icp.ic_replica_url,
    )?;
    info!(LOG, "ic_agent {:?}", ic_agent);

    info!(
        LOG,
        "icp canister id {:?}", &relayer_state.config.icp.canister_id
    );
    info!(
        LOG,
        "icp replica url {:?}", &relayer_state.config.icp.ic_replica_url
    );

    let oracle_client = DkimOracleClient::new(&relayer_state.config.icp.canister_id, &ic_agent)?;
    info!(LOG, "oracle_client {:?}", oracle_client);

    // Request signature from oracle
    let oracle_result = oracle_client.request_signature(&selector, &domain).await?;
    info!(LOG, "DKIM oracle result {:?}", oracle_result);

    // Process oracle response
    let public_key_hash = hex::decode(&oracle_result.public_key_hash[2..])?;
    info!(LOG, "public_key_hash from oracle {:?}", public_key_hash);
    let signature = Bytes::from_hex(&oracle_result.signature[2..])?;
    info!(LOG, "signature {:?}", signature);

    // Set DKIM public key hash
    let tx_hash = chain_client
        .set_dkim_public_key_hash(
            selector,
            domain,
            TryInto::<[u8; 32]>::try_into(public_key_hash).unwrap(),
            signature,
            dkim,
        )
        .await?;
    info!(LOG, "DKIM registry updated {:?}", tx_hash);
    Ok(())
}
