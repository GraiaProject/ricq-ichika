use std::sync::Arc;
use anyhow::Result;
use bytes::Bytes;
use tokio::net::TcpStream;
use tokio::time::{Duration, sleep};
use tokio_util::codec::{Framed, LengthDelimitedCodec};
use rs_qq::client::{Client, Password};
use rs_qq::client::net::ClientNet;

#[tokio::main]
async fn main() -> Result<()> {
    let (cli, receiver) = Client::new(
        0,
        Password::from_str(""),
    ).await;

    let client = Arc::new(cli);
    let client_net = ClientNet::new(client.clone(), receiver);
    let stream = client_net.connect_tcp().await;
    let net = tokio::spawn(client_net.net_loop(stream));
    net.await;
    let (seq, pkt) = client.build_qrcode_fetch_request_packet().await;
    sleep(Duration::from_millis(100)).await;
    Ok(())
    // client.login().await;
}
