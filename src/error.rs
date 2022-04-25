use solana_program::pubkey::Pubkey;

use crate::rpc::ErrorReply;

#[derive(Debug)]
pub enum Error {
    ReqwestError(reqwest::Error),
    BincodeError(bincode::Error),
    ParseSignatureError(solana_sdk::signature::ParseSignatureError),
    MismatchedSignatureError {
        transaction: solana_sdk::signature::Signature,
        response: solana_sdk::signature::Signature,
    },
    SolanaParseHashError(solana_program::hash::ParseHashError),
    RPCError(ErrorReply),
    UnknownRPCReply(String),
    UrlParseError(url::ParseError),
    TungsteniteError(tokio_tungstenite::tungstenite::Error),
    PubsubDied,
    MissingAccountEncoding,
    InvalidAccountEncoding(String),
    MissingAccountData,
    Base64DecodeError(base64::DecodeError),
    IOError(std::io::Error),
    ParsePubkeyError(solana_sdk::pubkey::ParsePubkeyError),
    InvalidProgramAccount(Pubkey),
    TransactionError(solana_sdk::transaction::TransactionError),
}

impl From<reqwest::Error> for Error {
    fn from(err: reqwest::Error) -> Self {
        Self::ReqwestError(err)
    }
}

impl From<bincode::Error> for Error {
    fn from(err: bincode::Error) -> Self {
        Self::BincodeError(err)
    }
}

impl From<solana_sdk::signature::ParseSignatureError> for Error {
    fn from(err: solana_sdk::signature::ParseSignatureError) -> Self {
        Self::ParseSignatureError(err)
    }
}

impl From<solana_sdk::pubkey::ParsePubkeyError> for Error {
    fn from(err: solana_sdk::pubkey::ParsePubkeyError) -> Self {
        Self::ParsePubkeyError(err)
    }
}

impl From<solana_program::hash::ParseHashError> for Error {
    fn from(err: solana_program::hash::ParseHashError) -> Self {
        Self::SolanaParseHashError(err)
    }
}

impl From<url::ParseError> for Error {
    fn from(err: url::ParseError) -> Self {
        Self::UrlParseError(err)
    }
}

impl From<tokio_tungstenite::tungstenite::Error> for Error {
    fn from(err: tokio_tungstenite::tungstenite::Error) -> Self {
        Self::TungsteniteError(err)
    }
}

impl From<base64::DecodeError> for Error {
    fn from(err: base64::DecodeError) -> Self {
        Self::Base64DecodeError(err)
    }
}

impl From<std::io::Error> for Error {
    fn from(err: std::io::Error) -> Self {
        Self::IOError(err)
    }
}

impl From<solana_sdk::transaction::TransactionError> for Error {
    fn from(err: solana_sdk::transaction::TransactionError) -> Self {
        Self::TransactionError(err)
    }
}

impl std::error::Error for Error {}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}
