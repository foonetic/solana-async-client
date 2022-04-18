use crate::{
    error::Error,
    pubsub::{Pubsub, PubsubRequest},
    rpc::{
        AccountInfoReply, ErrorReply, IsBlockhashValidReply, LatestBlockhashReply,
        MinimumBalanceForRentExemptionReply, ProgramAccountsReply, SignatureStatusesReply,
        TransactionReply,
    },
};
use solana_program::{
    bpf_loader, bpf_loader_deprecated,
    bpf_loader_upgradeable::{self, UpgradeableLoaderState},
    hash::Hash,
};
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
#[derive(Debug)]
pub struct LatestBlockhash {
    pub slot: u64,
    pub hash: Hash,
    pub last_valid_block_height: u64,
}

/// Return value for get_account_info RPC call.
#[derive(Debug)]
pub struct AccountInfo {
    pub data: Vec<u8>,
    pub executable: bool,
    pub lamports: u64,
    pub owner: Pubkey,
    pub rent_epoch: u64,
}

/// Request a specific slice of data from an account.
pub struct DataSlice {
    pub offset: usize,
    pub length: usize,
}

pub enum AccountFilter {
    DataSize(usize),
    MemCmp { offset: usize, bytes: Vec<u8> },
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

    /// Returns an account. If a data slice is provided, then only the given
    /// subset of the account is returned.
    pub async fn get_account_info(
        &self,
        pubkey: &Pubkey,
        commitment: CommitmentLevel,
        data_slice: Option<&DataSlice>,
    ) -> Result<Option<AccountInfo>, Error> {
        let body = format!(
            r#"{{"jsonrpc":"2.0","id":1,"method":"getAccountInfo","params":["{}",{{"commitment":"{}","encoding":"base64+zstd"{}}}]}}"#,
            pubkey.to_string(),
            commitment.to_string(),
            if let Some(data_slice) = data_slice {
                format!(
                    r#","dataSlice":{{"offset":{},"length":{}}}"#,
                    data_slice.offset, data_slice.length
                )
            } else {
                "".to_string()
            }
        );
        let response = self
            .http
            .post(&self.url)
            .body(body)
            .header("Content-Type", "application/json")
            .send()
            .await?
            .text()
            .await?;

        let response = parse_json::<AccountInfoReply>(&response)?;

        if let Some(value) = response.result.value {
            match value.data.get(1) {
                None => {
                    return Err(Error::MissingAccountEncoding);
                }
                Some(encoding) => {
                    if encoding != "base64+zstd" {
                        return Err(Error::InvalidAccountEncoding(encoding.clone()));
                    }
                    match value.data.get(0) {
                        None => {
                            return Err(Error::MissingAccountData);
                        }
                        Some(data) => {
                            let decoded = base64::decode(data)?;
                            let data = zstd::decode_all(decoded.as_slice())?;
                            Ok(Some(AccountInfo {
                                data,
                                executable: value.executable,
                                lamports: value.lamports,
                                owner: value.owner.parse()?,
                                rent_epoch: value.rent_epoch,
                            }))
                        }
                    }
                }
            }
        } else {
            Ok(None)
        }
    }

    /// Returns the accounts belonging to the given program. Optionally return a
    /// subset of the account data. Optionally filter by account size and data
    /// contents.
    pub async fn get_program_accounts(
        &self,
        program: &Pubkey,
        commitment: CommitmentLevel,
        data_slice: Option<&DataSlice>,
        filters: Option<&[AccountFilter]>,
    ) -> Result<Vec<(Pubkey, AccountInfo)>, Error> {
        let response = self
            .http
            .post(&self.url)
            .body(format!(
                r#"{{"jsonrpc":"2.0","id":1,"method":"getProgramAccounts","params":["{}",{{"commitment":"{}","encoding":"base64+zstd"{}{}}}]}}"#,
                program.to_string(),
                commitment.to_string(),
                if let Some(data_slice) = data_slice {
                    format!(r#","dataSlice":{{"offset":{},"length":{}}}"#, data_slice.offset, data_slice.length)
                } else {
                    "".to_string()
                },
                if let Some(filters) = filters {
                    let formatted = filters.iter().map(|f| match f {
                        AccountFilter::DataSize(size) => format!(r#"{{"dataSize":{}}}"#, size),
                        AccountFilter::MemCmp{offset, bytes} => {
                            let encoded = bs58::encode(bytes).into_string();
                            format!(r#"{{"memcmp":{{"offset":{},"bytes":"{}"}}"#, offset, encoded)
                        }
                    }).collect::<Vec<String>>().join(",");
                    format!(r#","filters":[{}]"#, formatted)
                } else {
                    "".to_string()
                }
            ))
            .header("Content-Type", "application/json")
            .send()
            .await?
            .text()
            .await?;

        let response = parse_json::<ProgramAccountsReply>(&response)?;

        let mut accounts = Vec::new();
        for account in response.result.iter() {
            match account.account.data.get(1) {
                None => {
                    return Err(Error::MissingAccountEncoding);
                }
                Some(encoding) => {
                    if encoding != "base64+zstd" {
                        return Err(Error::InvalidAccountEncoding(encoding.clone()));
                    }
                }
            }

            match account.account.data.get(0) {
                None => {
                    return Err(Error::MissingAccountData);
                }
                Some(data) => {
                    let decoded = base64::decode(data)?;
                    let data = zstd::decode_all(decoded.as_slice())?;

                    accounts.push((
                        account.pubkey.parse()?,
                        AccountInfo {
                            data,
                            executable: account.account.executable,
                            lamports: account.account.lamports,
                            owner: account.account.owner.parse()?,
                            rent_epoch: account.account.rent_epoch,
                        },
                    ));
                }
            }
        }

        Ok(accounts)
    }

    /// Returns the BPF code corresponding to a deployed program.
    pub async fn get_program_bpf(
        &self,
        program: &Pubkey,
        commitment: CommitmentLevel,
    ) -> Result<Option<Vec<u8>>, Error> {
        match self.get_account_info(program, commitment, None).await? {
            None => {
                return Ok(None);
            }
            Some(account) => {
                if account.owner == bpf_loader::id() || account.owner == bpf_loader_deprecated::id()
                {
                    return Ok(Some(account.data));
                } else if account.owner == bpf_loader_upgradeable::id() {
                    if let Ok(UpgradeableLoaderState::Program {
                        programdata_address,
                    }) = bincode::deserialize(&account.data)
                    {
                        if let Ok(Some(programdata_account)) = self
                            .get_account_info(&programdata_address, commitment, None)
                            .await
                        {
                            if let Ok(UpgradeableLoaderState::ProgramData { .. }) =
                                bincode::deserialize(&programdata_account.data)
                            {
                                let offset =
                                    UpgradeableLoaderState::programdata_data_offset().unwrap_or(0);
                                return Ok(Some(programdata_account.data[offset..].to_vec()));
                            }
                        }
                    }
                }

                return Err(Error::InvalidProgramAccount(program.clone()));
            }
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
