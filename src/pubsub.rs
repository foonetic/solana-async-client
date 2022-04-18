use crate::{
    error::Error,
    rpc::{SignatureNotification, SubscriptionReply},
};
use futures_util::{
    stream::{SplitSink, SplitStream},
    SinkExt, StreamExt,
};
use solana_sdk::{commitment_config::CommitmentLevel, signature::Signature};
use std::{collections::HashMap, time::Duration};
use tokio::{
    net::TcpStream,
    sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender},
};
use tokio_tungstenite::{connect_async, tungstenite::Message, MaybeTlsStream, WebSocketStream};
use url::Url;

/// Represents a pubsub interface.
pub struct Pubsub {
    next_request: u64,
    enqueue_write: UnboundedSender<Message>,
    commands: UnboundedReceiver<PubsubRequest>,
    read: SplitStream<WebSocketStream<MaybeTlsStream<TcpStream>>>,
    reader_receiever: UnboundedReceiver<SplitStream<WebSocketStream<MaybeTlsStream<TcpStream>>>>,
    disconnect_sender: UnboundedSender<()>,

    subscription_to_id: HashMap<u64, u64>,
    signature_notifiers: HashMap<u64, UnboundedSender<bool>>,
}

/// A request sent by the RpcClient.
pub enum PubsubRequest {
    /// Subscribe to signature updates at the given commitment level. On reply,
    /// send the confirmation status back on the provided channel.
    SignatureSubscribe(Signature, CommitmentLevel, UnboundedSender<bool>),
}

impl Pubsub {
    pub async fn new(
        endpoint: Url,
        commands: UnboundedReceiver<PubsubRequest>,
    ) -> Result<Self, Error> {
        let (socket, _) = connect_async(endpoint.clone()).await?;
        let (write, read) = socket.split();

        let (enqueue_write, dequeue_write) = unbounded_channel();
        let (disconnect_sender, disconnect_receiever) = unbounded_channel();

        let (writer_sender, writer_receiever) = unbounded_channel();
        let (reader_sender, reader_receiever) = unbounded_channel();

        let mut pinger = PubsubPinger {
            enqueue_write: enqueue_write.clone(),
        };
        tokio::spawn(async move {
            pinger.run().await;
        });

        let mut disconnect = PubsubDisconnect {
            endpoint,
            disconnect_receiever,
            reader_sender,
            writer_sender,
        };
        tokio::spawn(async move {
            disconnect.run().await;
        });

        let mut writer = PubsubWriter {
            write,
            writer_receiever,
            dequeue_write,
            disconnect_sender: disconnect_sender.clone(),
        };
        tokio::spawn(async move {
            writer.run().await;
        });

        Ok(Self {
            next_request: 0,
            enqueue_write,
            commands,
            read,
            reader_receiever,
            disconnect_sender: disconnect_sender.clone(),
            subscription_to_id: HashMap::new(),
            signature_notifiers: HashMap::new(),
        })
    }

    /// Increments the request counter and returns the next request ID to use.
    fn next_request(&mut self) -> u64 {
        let next = self.next_request;
        self.next_request = self.next_request.wrapping_add(1);
        next
    }

    /// Async run loop for the pubsub client.
    pub async fn run(&mut self) {
        loop {
            tokio::select! {
                Some(read) = self.reader_receiever.recv() => {
                    self.read = read;
                }

                Some(message) = self.read.next() => {
                    match message {
                        Err(_) => {
                            // We disconnected. Notify the monitor.
                            if let Err(_) = self.disconnect_sender.send(()) {
                                // On disconnect, stop the loop.
                                return;
                            }

                            // Attempt to reconnect.
                            if let Some(read) = self.reader_receiever.recv().await {
                                self.read = read;
                            } else {
                                // On disconnect, stop the loop.
                                return;
                            }
                        }
                        Ok(message) => {
                            self.on_message(message);
                        }
                    }
                }

                Some(command) = self.commands.recv() => {
                    if let Err(_) = self.on_command(command) {
                        // The downstream command signaled a disconnect.
                        return;
                    }
                }
            }
        }
    }

    fn on_message(&mut self, message: Message) {
        match message {
            Message::Text(text) => {
                if let Ok(reply) = serde_json::from_str::<SubscriptionReply>(&text) {
                    self.subscription_to_id.insert(reply.result, reply.id);
                } else if let Ok(reply) = serde_json::from_str::<SignatureNotification>(&text) {
                    if let Some(id) = self.subscription_to_id.remove(&reply.params.subscription) {
                        if let Some(notify) = self.signature_notifiers.remove(&id) {
                            if let Err(_) = notify.send(reply.params.result.value.err.is_null()) {
                                // Do nothing since the channel is dead. This
                                // isn't necessarily an error since the
                                // notification may have already been receieved
                                // on another channel.
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }

    fn on_command(&mut self, command: PubsubRequest) -> Result<(), ()> {
        match command {
            PubsubRequest::SignatureSubscribe(signature, commitment, sender) => {
                let id = self.next_request();
                self.signature_notifiers.insert(id, sender);

                if let Err(_) = self.enqueue_write.send(
                    Message::Text(
                        format!(
                            r#"{{"jsonrpc":"2.0","id":{},"method":"signatureSubscribe","params":["{}",{{"commitment":"{}"}}]}}"#, 
                            id, signature.to_string(), commitment.to_string()))) {
                    return Err(());
                }
            }
        }
        Ok(())
    }
}

/// Pings the pubsub endpoint to keep the connection alive.
struct PubsubPinger {
    enqueue_write: UnboundedSender<Message>,
}

impl PubsubPinger {
    async fn run(&mut self) {
        let mut interval = tokio::time::interval(Duration::from_secs(30));
        while let Ok(_) = self.enqueue_write.send(Message::Ping(b"ping".to_vec())) {
            interval.tick().await;
        }
    }
}

/// Handles disconnect signals from readers and writers. Reconnect and send the
/// new readers and writers.
struct PubsubDisconnect {
    endpoint: Url,
    disconnect_receiever: UnboundedReceiver<()>,
    reader_sender: UnboundedSender<SplitStream<WebSocketStream<MaybeTlsStream<TcpStream>>>>,
    writer_sender: UnboundedSender<SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, Message>>,
}

impl PubsubDisconnect {
    async fn run(&mut self) {
        while let Some(_) = self.disconnect_receiever.recv().await {
            let mut sleep_seconds = 1;
            loop {
                let result = connect_async(&self.endpoint).await;
                if let Ok((socket, _)) = result {
                    let (write, read) = socket.split();
                    if let Err(_) = self.writer_sender.send(write) {
                        return;
                    }
                    if let Err(_) = self.reader_sender.send(read) {
                        return;
                    }
                    break;
                }

                // Retry with exponential backoff.
                tokio::time::sleep(Duration::from_secs(sleep_seconds)).await;
                if sleep_seconds < 32 {
                    sleep_seconds *= 2;
                }
            }
        }
    }
}

/// Writes data to the websocket.
struct PubsubWriter {
    write: SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, Message>,
    writer_receiever:
        UnboundedReceiver<SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, Message>>,
    dequeue_write: UnboundedReceiver<Message>,
    disconnect_sender: UnboundedSender<()>,
}

impl PubsubWriter {
    async fn run(&mut self) {
        loop {
            tokio::select! {
                Some(write) = self.writer_receiever.recv() => {
                    self.write = write;
                }

                Some(message) = self.dequeue_write.recv() => {
                    if let Err(_) = self.write.send(message).await {
                        // The channel is closed, so we should shut down.
                        if let Err(_) = self.disconnect_sender.send(()) {
                            return;
                        }
                        // Wait for a new writer.
                        if let Some(write) = self.writer_receiever.recv().await {
                            self.write = write;
                        } else {
                            return;
                        }
                    }
                }
            }
        }
    }
}
