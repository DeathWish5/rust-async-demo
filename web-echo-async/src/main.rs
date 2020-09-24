use tokio::net::{TcpListener, TcpStream};
use tokio::prelude::*;

async fn handle_client(mut stream: TcpStream) -> Result<(), Box<dyn std::error::Error>> {
    println!("Connect form {}", stream.peer_addr().unwrap());
    let mut buffer = [0u8; 128];
    let len = stream.read(&mut buffer).await?;
    let message = if &buffer[..len] == b"hello\r\n" {
        "hi!\r\n"
    } else {
        "who are you?\r\n"
    };
    stream.write(message.as_bytes()).await?;
    println!("Connect form {} end", stream.peer_addr().unwrap());
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut listener = TcpListener::bind("127.0.0.1:8080").await?;
    loop {
        let (stream, _) = listener.accept().await?;
        tokio::spawn(async move {
            handle_client(stream).await.unwrap()
        });
    }
}