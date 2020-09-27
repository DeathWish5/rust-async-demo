use mio::tcp::{TcpListener, TcpStream};
use mio::*;
use std::collections::HashMap;
use std::io::{Read, Write};

const SERVER: Token = Token(0);

fn main() {
    let mut events = Events::with_capacity(100);
    let poll = Poll::new().unwrap();
    let addr = "127.0.0.1:8080".parse().unwrap();
    let server = TcpListener::bind(&addr).unwrap();
    poll.register(&server, SERVER, Ready::readable(), PollOpt::edge())
        .unwrap();
    let mut map = HashMap::<usize, TcpStream>::new();
    let mut stream_id = 1usize;
    loop {
        poll.poll(&mut events, None).unwrap();
        for event in events.iter() {
            match event.token() {
                SERVER => match server.accept() {
                    Ok((stream, _)) => {
                        println!("Connect form {}", stream.peer_addr().unwrap());
                        poll.register(
                            &stream,
                            Token(stream_id),
                            Ready::readable(),
                            PollOpt::edge(),
                        )
                        .unwrap();
                        map.insert(stream_id, stream);
                        stream_id += 1;
                    }
                    _ => panic!("????"),
                },
                Token(id) => {
                    let mut stream = map.remove(&id).unwrap();
                    let mut buffer = [0; 512];
                    let len = stream.read(&mut buffer).unwrap();
                    let message = if &buffer[..len] == b"hello\r\n" {
                        "hi!\r\n"
                    } else {
                        "who are you?\r\n"
                    };
                    stream.write(message.as_bytes()).unwrap();
                    println!("Connect form {} end", stream.peer_addr().unwrap());
                }
            }
        }
    }
}
