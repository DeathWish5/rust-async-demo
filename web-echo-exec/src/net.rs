use core::{
    future::Future,
    pin::Pin,
    sync::atomic::{AtomicUsize, Ordering},
    task::{Context, Poll, Waker},
};
use lazy_static::*;
use mio::event::Evented;
use mio::tcp::{TcpListener, TcpStream};
use mio::{Events, Poll as MPoll, PollOpt, Ready, Token};
use std::{
    collections::HashMap,
    io::{Read, Result, Write},
    net::SocketAddr,
    sync::{Arc, Mutex},
};

struct EventId(usize);

impl EventId {
    fn new() -> usize {
        static NEXT_ID: AtomicUsize = AtomicUsize::new(1);
        NEXT_ID.fetch_add(1, Ordering::Relaxed)
    }
}

pub struct MioReactor {
    events: Events,
    poll: MPoll,
    map: HashMap<usize, Waker>,
}

impl MioReactor {
    pub fn new() -> Self {
        MioReactor {
            events: Events::with_capacity(128),
            poll: MPoll::new().unwrap(),
            map: HashMap::new(),
        }
    }

    pub fn register<E: mio::Evented>(
        &mut self,
        handle: &E,
        token: Token,
        interest: Ready,
        opts: PollOpt,
    ) -> () {
        self.poll.register(handle, token, interest, opts).unwrap();
    }

    pub fn register_waker(&mut self, token: Token, waker: Waker) {
        self.map.insert(token.into(), waker);
    }

    pub fn self_poll(&mut self) -> usize {
        self.poll.poll(&mut self.events, None).unwrap()
    }

    pub fn poll(reactor: Arc<Mutex<Self>>) -> impl Future<Output = ()> {
        struct MPollFuture {
            reactor: Arc<Mutex<MioReactor>>,
        }

        impl Future for MPollFuture {
            type Output = ();

            fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
                let mut reactor = self.reactor.lock().unwrap();
                reactor.self_poll();
                let waker_id: Vec<usize> =
                    reactor.events.iter().map(|event| event.token().0).collect();
                for id in waker_id {
                    let waker = reactor.map.remove(&id);
                    if let Some(waker) = waker {
                        waker.wake_by_ref();
                    }
                }
                drop(reactor);
                let waker = cx.waker().clone();
                waker.wake_by_ref();
                Poll::Pending
            }
        }

        MPollFuture {
            reactor: Arc::clone(&reactor),
        }
    }
}

lazy_static! {
    pub static ref MIOREACTOR: Arc<Mutex<MioReactor>> = {
        let m = MioReactor::new();
        Arc::new(Mutex::new(m))
    };
}

pub async fn poll() {
    let reactor = Arc::clone(&MIOREACTOR);
    MioReactor::poll(reactor).await;
    unreachable!();
}

pub struct SimpleTcpListener {
    token: Token,
    io: Mutex<TcpListener>,
    registered: Mutex<bool>,
}

pub struct SimpleTcpStream {
    token: Token,
    io: Mutex<TcpStream>,
    registered: Mutex<bool>,
}

impl Evented for SimpleTcpStream {
    fn register(&self, poll: &MPoll, token: Token, interest: Ready, opts: PollOpt) -> Result<()> {
        let io = self.io.lock().unwrap();
        io.register(poll, token, interest, opts)
    }

    fn reregister(&self, poll: &MPoll, token: Token, interest: Ready, opts: PollOpt) -> Result<()> {
        let io = self.io.lock().unwrap();
        io.reregister(poll, token, interest, opts)
    }

    fn deregister(&self, poll: &MPoll) -> Result<()> {
        let io = self.io.lock().unwrap();
        io.deregister(poll)
    }
}

impl Evented for SimpleTcpListener {
    fn register(&self, poll: &MPoll, token: Token, interest: Ready, opts: PollOpt) -> Result<()> {
        let io = self.io.lock().unwrap();
        io.register(poll, token, interest, opts)
    }

    fn reregister(&self, poll: &MPoll, token: Token, interest: Ready, opts: PollOpt) -> Result<()> {
        let io = self.io.lock().unwrap();
        io.reregister(poll, token, interest, opts)
    }

    fn deregister(&self, poll: &MPoll) -> Result<()> {
        let io = self.io.lock().unwrap();
        io.deregister(poll)
    }
}

impl SimpleTcpStream {
    fn self_register(self: &Self) {
        let mut registered = self.registered.lock().unwrap();
        if(*registered == false) {
            *registered = true;
            let mut reactor = MIOREACTOR.lock().unwrap();
            reactor.register(
                self,
                self.token,
                Ready::readable(),
                PollOpt::edge(),
            );
        }
    }

    fn waker_register(self: &Self, cx: &mut Context<'_>) {
        let mut reactor = MIOREACTOR.lock().unwrap();
        reactor.register_waker(self.token, cx.waker().clone());
    }

    fn read(self: &Self, buffer: &mut [u8]) -> Result<usize> {
        let mut io = self.io.lock().unwrap();
        io.read(buffer)
    }

    fn write(self: &Self, buffer: &[u8]) -> Result<usize> {
        let mut io = self.io.lock().unwrap();
        io.write(buffer)
    }

    pub fn peer_addr(self: &Self) -> SocketAddr {
        let io = self.io.lock().unwrap();
        io.peer_addr().unwrap()
    }

    pub fn async_read<'a>(
        self: &Arc<Self>,
        buffer: &'a mut [u8],
    ) -> impl Future<Output = usize> + 'a {
        struct ReadFuture<'a> {
            stream: Arc<SimpleTcpStream>,
            buffer: &'a mut [u8],
        }

        impl Future for ReadFuture<'_> {
            type Output = usize;

            fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
                let mut buf = [0u8; 1024];
                if let Ok(len) = self.stream.read(&mut buf) {
                    let length = self.buffer.len();
                    self.buffer.copy_from_slice(&mut buf[..length]);
                    return Poll::Ready(len);
                }
                self.stream.waker_register(cx);
                Poll::Pending
            }
        }

        ReadFuture {
            stream: Arc::clone(self),
            buffer: buffer,
        }
    }

    pub fn async_write<'a>(self: &Arc<Self>, buffer: &'a [u8]) -> impl Future<Output = usize> + 'a {
        struct WriteFuture<'a> {
            stream: Arc<SimpleTcpStream>,
            buffer: &'a [u8],
        }

        impl Future for WriteFuture<'_> {
            type Output = usize;

            fn poll(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
                let mut buf = [0u8; 1024];
                buf[..self.buffer.len()].copy_from_slice(self.buffer);
                let len = self.stream.write(&buf).unwrap();
                Poll::Ready(len)
            }
        }

        WriteFuture {
            stream: Arc::clone(self),
            buffer: buffer,
        }
    }
}

impl SimpleTcpListener {
    fn self_register(self: &Self) {
        let mut registered = self.registered.lock().unwrap();
        if(*registered == false) {
            *registered = true;
            let mut reactor = MIOREACTOR.lock().unwrap();
            reactor.register(
                self,
                self.token,
                Ready::readable(),
                PollOpt::edge(),
            );
        }
    }

    fn waker_register(self: &Self, cx: &mut Context<'_>) {
        let mut reactor = MIOREACTOR.lock().unwrap();
        reactor.register_waker(self.token, cx.waker().clone());
    }

    fn accept(self: &Self) -> Result<(TcpStream, SocketAddr)> {
        let io = self.io.lock().unwrap();
        io.accept()
    }

    pub fn async_bind<'a>(addr: &'a SocketAddr) -> impl Future<Output = SimpleTcpListener> + 'a {
        struct BindFuture<'a> {
            addr: &'a SocketAddr,
        }

        impl Future for BindFuture<'_> {
            type Output = SimpleTcpListener;

            fn poll(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
                let server = TcpListener::bind(&self.addr).unwrap();
                let listener = SimpleTcpListener {
                    token: Token(EventId::new()),
                    io: Mutex::new(server),
                    registered: Mutex::new(false),
                };
                listener.self_register();
                Poll::Ready(listener)
            }
        }

        BindFuture { addr }
    }

    pub fn async_accept(self: &Arc<Self>) -> impl Future<Output = SimpleTcpStream> {
        struct AcceptFuture {
            listener: Arc<SimpleTcpListener>,
        }

        impl Future for AcceptFuture {
            type Output = SimpleTcpStream;

            fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
                if let Ok((stream, _)) = self.listener.accept() {
                    let stream = SimpleTcpStream {
                        token: Token(EventId::new()),
                        io: Mutex::new(stream),
                        registered: Mutex::new(false),
                    };
                    stream.self_register();
                    return Poll::Ready(stream);
                }
                self.listener.waker_register(cx);
                Poll::Pending
            }
        }

        AcceptFuture {
            listener: Arc::clone(self),
        }
    }
}
