use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;

fn handle_client(mut stream: TcpStream) -> std::io::Result<()> {
    println!("Connect form {}", stream.peer_addr().unwrap());
    let mut buffer = [0; 512];
    let len = stream.read(&mut buffer)?;
    let message = if &buffer[..len] == b"hello\r\n" {
        "hi!\r\n"
    } else {
        "who are you?\r\n"
    };
    stream.write(message.as_bytes())?;
    println!("Connect form {} end", stream.peer_addr().unwrap());
    Ok(())
}

fn singal_accept() -> std::io::Result<()> {
    let listener = TcpListener::bind("127.0.0.1:8080")?;
    while let Ok((stream, _)) = listener.accept() {
        handle_client(stream)?;
    }
    Ok(())
}

fn thread_accept() -> std::io::Result<()> {
    let listener = TcpListener::bind("127.0.0.1:8080")?;
    while let Ok((stream, _)) = listener.accept() {
        thread::spawn(move || {
            handle_client(stream).unwrap();
        });
    }
    Ok(())
}

fn main() {
    // singal_accept().unwrap();
    thread_accept().unwrap();
}
