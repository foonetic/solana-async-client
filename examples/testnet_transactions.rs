/// Client demo that airdrops and sends a transaction on testnet.
use solana_async_client::client::{PubsubAPI, RestAPI, RpcClient};
use solana_program::system_instruction::create_account;
use solana_sdk::{
    commitment_config::CommitmentLevel, signature::Keypair, signer::Signer,
    transaction::Transaction,
};
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = RpcClient::new_with_timeout(
        RestAPI("https://api.testnet.solana.com".to_string()),
        PubsubAPI("wss://api.testnet.solana.com".to_string()),
        Duration::from_secs(30),
    )
    .await?;

    let keypair = Keypair::new();
    println!("Pubkey = {}", keypair.pubkey());

    let airdrop = client
        .request_airdrop(&keypair.pubkey(), 1_000_000_000, CommitmentLevel::Confirmed)
        .await?;
    println!("Airdrop signature = {}", airdrop);

    let new_account = Keypair::new();

    let lamports = client.get_minimum_balance_for_rent_exemption(5).await?;
    println!("Lamports needed: {}", lamports);

    let instruction = create_account(
        &keypair.pubkey(),
        &new_account.pubkey(),
        lamports,
        5,
        &keypair.pubkey(),
    );
    let recent_blockhash = client
        .get_latest_blockhash(CommitmentLevel::Confirmed)
        .await?;
    println!("Recent blockhash = {}", recent_blockhash.hash);

    let transaction = Transaction::new_signed_with_payer(
        &[instruction],
        Some(&keypair.pubkey()),
        &[&keypair, &new_account],
        recent_blockhash.hash,
    );
    let signature = client
        .send_and_confirm_transaction(&transaction, CommitmentLevel::Confirmed)
        .await?;

    println!("Signature: {}", signature);

    let account_info = client
        .get_account_info(&new_account.pubkey(), CommitmentLevel::Confirmed, None)
        .await?;
    println!("Account Info: {:?}", account_info);

    let program_accounts = client
        .get_program_accounts(&keypair.pubkey(), CommitmentLevel::Confirmed, None, None)
        .await?;
    println!("Program accounts: {:?}", program_accounts);

    let bpf = client
        .get_program_bpf(&spl_token::ID, CommitmentLevel::Confirmed)
        .await?;
    println!(
        "Token program bytes: {}",
        bpf.unwrap_or_else(Vec::new).len()
    );

    Ok(())
}
