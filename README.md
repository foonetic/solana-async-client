# Solana Async Client

This package provides a Solana RpcClient clone that exclusively supports tokio.
Transaction confirmations automatically arbitrate between periodic polling and
the websocket signature subscription.

See `examples/testnet_transactions.rs` for a demo of the client.