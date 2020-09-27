use simple_executor as executor;
use std::{sync::Arc, io::{Read, Write}, net::SocketAddr};
use mio::tcp::{TcpListener, TcpStream};

pub mod net;

async fn handle_client(stream: net::SimpleTcpStream) {
    println!("Connect form {}", stream.peer_addr().unwrap());
    let stream = Arc::new(stream);
    let mut buffer = [0u8; 128];
    let len = stream.async_read(&mut buffer).await;
    let message = if &buffer[..len] == b"hello\r\n" {
        "hi!\r\n"
    } else {
        "who are you?\r\n"
    };
    stream.async_write(message.as_bytes()).await;
    println!("Connect form {} end", stream.peer_addr().unwrap());
}

async fn server() {
    let addr = "127.0.0.1:8080".parse().unwrap();
    let listener = net::SimpleTcpListener::async_bind(&addr).await;
    let listener = Arc::new(listener);
    loop {
        let stream = listener.async_accept().await;
        executor::spawn(handle_client(stream));
    }
}

fn main() {
    executor::spawn(server());
    let reactor = net::MIOREACTOR.lock().unwrap();
    executor::spawn(reactor.poll());
    drop(reactor);
    executor::run();
}