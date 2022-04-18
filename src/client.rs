use crate::{
    error::Error,
    pubsub::{Pubsub, PubsubRequest},
    rpc::{
        ErrorReply, IsBlockhashValidReply, LatestBlockhashReply,
        MinimumBalanceForRentExemptionReply, SignatureStatusesReply, TransactionReply,
    },
};
use solana_program::hash::Hash;
use solana_sdk::{
    commitment_config::{CommitmentConfig, CommitmentLevel},
    pubkey::Pubkey,
    signature::Signature,
    transaction::Transaction,
};
use std::time::Duration;
use tokio::{
    sync::mpsc::{unbounded_channel, UnboundedSender},
    time::sleep,
};
use url::Url;

/// Address to a JSON RPC endpoint.
pub struct RestAPI(pub String);

/// Address to a Pubsub endpoint.
pub struct PubsubAPI(pub String);

/// JSON RPC client.
pub struct RpcClient {
    http: reqwest::Client,
    url: String,
    send_command: UnboundedSender<PubsubRequest>,
}

/// Return value for the get_latest_blockhash RPC call.
pub struct LatestBlockhash {
    pub slot: u64,
    pub hash: Hash,
    pub last_valid_block_height: u64,
}

impl RpcClient {
    /// Constructs a client that connects to the given rest and pubsub
    /// endpoints. HTTP requests are assumed to time out after the given
    /// timeout.
    pub async fn new_with_timeout(
        sync: RestAPI,
        pubsub: PubsubAPI,
        timeout: Duration,
    ) -> Result<Self, Error> {
        let url = Url::parse(&pubsub.0)?;

        let (send_command, receieve_command) = unbounded_channel();
        let mut pubsub = Pubsub::new(url, receieve_command).await?;
        tokio::spawn(async move {
            pubsub.run().await;
        });

        Ok(Self {
            http: reqwest::Client::builder().timeout(timeout).build()?,
            url: sync.0,
            send_command,
        })
    }

    /// Request an airdrop to the given account.
    pub async fn request_airdrop(
        &self,
        account: &Pubkey,
        lamports: u64,
        commitment: CommitmentLevel,
    ) -> Result<Signature, Error> {
        let response = self
            .http
            .post(&self.url)
            .header("Content-Type", "application/json")
            .body(format!(
                r#"{{"jsonrpc":"2.0","id":1,"method":"requestAirdrop","params":["{}",{},{{"commitment":"{}"}}]}}"#,
                account.to_string(),
                lamports,
                commitment.to_string(),
            ))
            .header("Content-Type", "application/json")
            .send()
            .await?
            .text()
            .await?;

        let response = parse_json::<TransactionReply>(&response)?;
        Ok(response.result.parse()?)
    }

    /// Returns the balance required for rent exemption for an account of the
    /// given number of bytes.
    pub async fn get_minimum_balance_for_rent_exemption(
        &self,
        length: usize,
    ) -> Result<u64, Error> {
        let response = self
            .http
            .post(&self.url)
            .header("Content-Type", "application/json")
            .body(format!(
                r#"{{"jsonrpc":"2.0","id":1,"method":"getMinimumBalanceForRentExemption","params":[{}]}}"#,
                length
            ))
            .header("Content-Type", "application/json")
            .send()
            .await?
            .text()
            .await?;

        let response = parse_json::<MinimumBalanceForRentExemptionReply>(&response)?;
        Ok(response.result)
    }

    /// Returns the latest blockhash.
    pub async fn get_latest_blockhash(
        &self,
        commitment: CommitmentLevel,
    ) -> Result<LatestBlockhash, Error> {
        let response = self
            .http
            .post(&self.url)
            .body(format!(
                r#"{{"jsonrpc":"2.0","id":1,"method":"getLatestBlockhash","params":[{{"commitment":"{}"}}]}}"#,
                commitment.to_string(),
            ))
            .header("Content-Type", "application/json")
            .send()
            .await?
            .text()
            .await?;

        let response = parse_json::<LatestBlockhashReply>(&response)?;

        Ok(LatestBlockhash {
            slot: response.result.context.slot,
            hash: response
                .result
                .value
                .blockhash
                .parse()
                .map_err(Error::from)?,
            last_valid_block_height: response.result.value.last_valid_block_height,
        })
    }

    /// Sends a transaction and returns the signature of that transaction.
    pub async fn send_transaction(&self, transaction: &Transaction) -> Result<Signature, Error> {
        let serialized = bincode::serialize(transaction)?;
        let encoded = base64::encode(&serialized);
        let response = self
            .http
            .post(&self.url)
            .body(format!(
                r#"{{"jsonrpc":"2.0","id":1,"method":"sendTransaction","params":["{}",{{"encoding":"base64","skipPreflight":true}}]}}"#,
                encoded
            ))
            .header("Content-Type", "application/json")
            .send()
            .await?
            .text()
            .await?;

        let response = parse_json::<TransactionReply>(&response)?;
        let signature = response.result.parse::<Signature>().map_err(Error::from)?;
        if signature != transaction.signatures[0] {
            Err(Error::MismatchedSignatureError {
                transaction: transaction.signatures[0],
                response: signature,
            })
        } else {
            Ok(transaction.signatures[0])
        }
    }

    /// Returns the statuses of the given transaction signatures.
    pub async fn get_signature_statuses(
        &self,
        signatures: &[Signature],
    ) -> Result<SignatureStatusesReply, Error> {
        let response = self
            .http
            .post(&self.url)
            .body(format!(
                r#"{{"jsonrpc":"2.0","id":1,"method":"getSignatureStatuses","params":[[{}]]}}"#,
                signatures
                    .iter()
                    .map(|s| format!(r#""{}""#, s.to_string()))
                    .collect::<Vec<String>>()
                    .join(",")
            ))
            .header("Content-Type", "application/json")
            .send()
            .await?
            .text()
            .await?;

        parse_json::<SignatureStatusesReply>(&response)
    }

    /// Returns true if the input blockhash is valid.
    pub async fn is_blockhash_valid(
        &self,
        hash: &Hash,
        commitment: CommitmentLevel,
    ) -> Result<bool, Error> {
        let response = self
            .http
            .post(&self.url)
            .body(format!(
                r#"{{"jsonrpc":"2.0","id":1,"method":"isBlockhashValid","params":["{}",{{"commitment":"{}"}}]}}"#,
                hash.to_string(),
                commitment.to_string(),
            ))
            .header("Content-Type", "application/json")
            .send()
            .await?
            .text()
            .await?;

        let response = parse_json::<IsBlockhashValidReply>(&response)?;
        Ok(response.result.value)
    }

    /// Waits until the signature is confirmed or cannot be confirmed. Returns
    /// true if successful and false if the transaction is in error state.
    pub async fn confirm_signature(
        &self,
        signature: Signature,
        commitment: CommitmentLevel,
    ) -> Result<bool, Error> {
        let recent_blockhash = self.get_latest_blockhash(commitment).await?;
        let (notify, mut receive) = unbounded_channel();
        self.send_command
            .send(PubsubRequest::SignatureSubscribe(
                signature.clone(),
                commitment.clone(),
                notify.clone(),
            ))
            .map_err(|_| Error::PubsubDied)?;

        let block = async {
            let mut interval = tokio::time::interval(Duration::from_secs(5));
            loop {
                interval.tick().await;
                if let Ok(status) = self.get_signature_statuses(&[signature]).await {
                    if let Some(Some(status)) = status.result.value.get(0) {
                        if status.satisfies_commitment(CommitmentConfig { commitment }) {
                            if let Err(_) = notify.send(true) {
                                // Ignore error.
                            }
                            break;
                        }
                    }
                    if let Ok(res) = self
                        .is_blockhash_valid(&recent_blockhash.hash, commitment)
                        .await
                    {
                        if !res {
                            if let Err(_) = notify.send(false) {
                                // Ignore error
                            }
                            break;
                        }
                    }
                }
            }
        };
        tokio::pin!(block);

        loop {
            tokio::select! {
                Some(confirmed) = receive.recv() => {
                    return Ok(confirmed);
                }
                _ = &mut block => {}
            }
        }
    }

    /// Retries a transaction until it is confirmed.
    pub async fn send_and_confirm_transaction(
        &self,
        transaction: &Transaction,
        commitment: CommitmentLevel,
    ) -> Result<Signature, Error> {
        loop {
            let signature = self.send_transaction(transaction).await?;
            if self.confirm_signature(signature, commitment).await? {
                return Ok(signature);
            }
            sleep(Duration::from_millis(400)).await;
        }
    }
}

/// Parses either the given type or an error type.
fn parse_json<'a, T: serde::Deserialize<'a>>(value: &'a str) -> Result<T, Error> {
    if let Ok(val) = serde_json::from_str(value) {
        return Ok(val);
    }

    if let Ok(val) = serde_json::from_str::<ErrorReply>(value) {
        return Err(Error::RPCError(val));
    }

    Err(Error::UnknownRPCReply(value.to_string()))
}
