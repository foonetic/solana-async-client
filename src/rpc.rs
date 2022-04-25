/// Represents JSON RPC types.
use serde::Deserialize;

#[derive(Deserialize, Debug)]
pub struct TransactionReply {
    pub result: String,
}

#[derive(Deserialize, Debug)]
pub struct ContextWithSlot {
    pub slot: u64,
}

#[derive(Deserialize, Debug)]
pub struct SignatureStatusesReply {
    pub result: SignatureStatusesReplyResult,
}

#[derive(Deserialize, Debug)]
pub struct SignatureStatusesReplyResult {
    pub context: ContextWithSlot,
    pub value: Vec<Option<solana_transaction_status::TransactionStatus>>,
}

#[derive(Deserialize, Debug)]
pub struct LatestBlockhashReply {
    pub result: LatestBlockhashReplyResult,
}

#[derive(Deserialize, Debug)]
pub struct LatestBlockhashReplyResult {
    pub context: ContextWithSlot,
    pub value: LatestBlockhashReplyResultValue,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct LatestBlockhashReplyResultValue {
    pub blockhash: String,
    pub last_valid_block_height: u64,
}

#[derive(Deserialize, Debug)]
pub struct IsBlockhashValidReply {
    pub result: IsBlockhashValidReplyResult,
}

#[derive(Deserialize, Debug)]
pub struct IsBlockhashValidReplyResult {
    pub context: ContextWithSlot,
    pub value: bool,
}

#[derive(Deserialize, Debug)]
pub struct MinimumBalanceForRentExemptionReply {
    pub result: u64,
}

#[derive(Deserialize, Debug)]
pub struct ErrorReply {
    pub error: ErrorReplyError,
}

#[derive(Deserialize, Debug)]
pub struct ErrorReplyError {
    pub code: i64,
    pub message: String,
}

#[derive(Deserialize, Debug)]
pub struct SubscriptionReply {
    pub result: u64,
    pub id: u64,
}

#[derive(Deserialize, Debug)]
pub struct SignatureNotification {
    pub params: SignatureNotificationParams,
}

#[derive(Deserialize, Debug)]
pub struct SignatureNotificationParams {
    pub subscription: u64,
    pub result: SignatureNotificationParamsResult,
}

#[derive(Deserialize, Debug)]
pub struct SignatureNotificationParamsResult {
    pub context: ContextWithSlot,
    pub value: SignatureNotificationParamsResultValue,
}

#[derive(Deserialize, Debug)]
pub struct SignatureNotificationParamsResultValue {
    pub err: Option<solana_sdk::transaction::TransactionError>,
}

#[derive(Deserialize, Debug)]
pub struct AccountInfoReply {
    pub result: AccountInfoReplyResult,
}

#[derive(Deserialize, Debug)]
pub struct AccountInfoReplyResult {
    pub context: ContextWithSlot,
    pub value: Option<AccountInfoReplyResultValue>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct AccountInfoReplyResultValue {
    pub data: Vec<String>,
    pub executable: bool,
    pub lamports: u64,
    pub owner: String,
    pub rent_epoch: u64,
}

#[derive(Deserialize, Debug)]
pub struct ProgramAccountsReply {
    pub result: Vec<ProgramAccountsReplyResult>,
}

#[derive(Deserialize, Debug)]
pub struct ProgramAccountsReplyResult {
    pub account: AccountInfoReplyResultValue,
    pub pubkey: String,
}

#[derive(Deserialize, Debug)]
pub struct AccountNotification {
    pub params: AccountNotificationParams,
}

#[derive(Deserialize, Debug)]
pub struct AccountNotificationParams {
    pub result: AccountInfoReplyResult,
    pub subscription: u64,
}
