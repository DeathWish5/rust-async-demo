use std::io::{Read, Write};
use std::net::TcpStream;
use std::thread;
use std::time::Duration;

const TIME_BASE: u64 = 10;

fn main() {
    let messages = ["hello\r\n", "gogogo", "rust"];
    let mut handles = Vec::new();
    for i in 0..3 {
        let id = i;
        let message = messages[i].as_bytes();
        handles.push(thread::spawn(move || {
            let mut stream = TcpStream::connect("127.0.0.1:8080").unwrap();
            thread::sleep(Duration::from_millis((10 - 2 * i) as u64 * TIME_BASE));
            stream.write(message).unwrap();
            let mut buffer = [0u8; 512];
            stream.read(&mut buffer[..]).unwrap();
            println!(
                "thread {}:{} : {}",
                id,
                stream.local_addr().unwrap(),
                String::from_utf8_lossy(&buffer)
            );
        }));
        thread::sleep(Duration::from_millis(TIME_BASE));
    }
    for handle in handles {
        handle.join().unwrap();
    }
}
