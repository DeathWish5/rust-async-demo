use core::{
    future::Future,
    ops::Deref,
    pin::Pin,
    sync::atomic::{AtomicUsize, Ordering},
    task::{Context, Poll, Waker},
};
use lazy_static::*;
use mio::tcp::{TcpListener, TcpStream};
use mio::{Events, Poll as MPoll, PollOpt, Ready, Token};
use std::{
    collections::HashMap,
    io::{Read, Write},
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
        waker: Waker,
    ) -> () {
        self.poll.register(handle, token, interest, opts).unwrap();
        if let Some(_) = self.map.insert(token.into(), waker) {
            panic!("token remaped");
        }
    }

    pub fn self_poll(&mut self) -> usize {
        self.poll.poll(&mut self.events, None).unwrap()
    }

    pub fn poll(self: &mut Self>) -> impl Future<Output = ()> {
        struct MPollFuture {
            reactor: Arc<MioReactor>,
        }

        impl Future for MPollFuture {
            type Output = ();

            fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
                let reactor = Arc::get_mut(&mut self.reactor).unwrap();
                reactor.self_poll();
                for event in reactor.events.iter() {
                    match event.token() {
                        Token(id) => {
                            let waker = reactor.map.remove(&id).unwrap();
                            waker.wake_by_ref();
                        }
                    }
                }
                drop(reactor);
                let waker = cx.waker().clone();
                waker.wake_by_ref();
                Poll::Pending
            }
        }

        MPollFuture {
            reactor: Arc::clone(self),
        }
    }
}

lazy_static! {
    pub static ref MIOREACTOR: Arc<Mutex<MioReactor>> = {
        let m = MioReactor::new();
        Arc::new(Mutex::new(m))
    };
}

pub struct SimpleTcpListener {
    io: TcpListener,
}

pub struct SimpleTcpStream {
    io: TcpStream,
}

impl Deref for SimpleTcpListener {
    type Target = TcpListener;

    fn deref(&self) -> &Self::Target {
        &self.io
    }
}

impl Deref for SimpleTcpStream {
    type Target = TcpStream;

    fn deref(&self) -> &Self::Target {
        &self.io
    }
}

impl SimpleTcpStream {
    fn register(self: &Self, cx: &mut Context<'_>) {
        let mut reactor = MIOREACTOR.lock().unwrap();
        reactor.register(
            &self.io,
            Token(EventId::new()),
            Ready::readable(),
            PollOpt::edge(),
            cx.waker().clone(),
        );
    }

    pub fn async_read<'a>(
        self: &Arc<Self>,
        buffer: &'a mut [u8],
    ) -> impl Future<Output = usize> + 'a {
        struct ReadFuture<'a> {
            stream: Arc<SimpleTcpStream>,
            buffer: &'a mut [u8],
            first: bool,
        }

        impl Future for ReadFuture<'_> {
            type Output = usize;

            fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
                let mut stream = Arc::get_mut(&mut self.stream).unwrap();
                if let Ok(len) = stream.io.read(self.buffer) {
                    return Poll::Ready(len);
                }
                if self.first {
                    self.stream.register(cx);
                    self.first = false;
                }
                Poll::Pending
            }
        }

        ReadFuture {
            stream: Arc::clone(self),
            buffer: buffer,
            first: false,
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
                let mut stream = Arc::get_mut(&mut self.stream).unwrap();
                let len = stream.io.write(self.buffer).unwrap();
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
    fn register(self: &Self, cx: &mut Context<'_>) {
        let mut reactor = MIOREACTOR.lock().unwrap();
        reactor.register(
            &self.io,
            Token(EventId::new()),
            Ready::readable(),
            PollOpt::edge(),
            cx.waker().clone(),
        );
    }

    pub fn async_bind<'a>(addr: &'a SocketAddr) -> impl Future<Output = SimpleTcpListener> + 'a {
        struct BindFuture<'a> {
            addr: &'a SocketAddr,
        }

        impl Future for BindFuture<'_> {
            type Output = SimpleTcpListener;

            fn poll(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
                let server = TcpListener::bind(&self.addr).unwrap();
                Poll::Ready(SimpleTcpListener { io: server })
            }
        }

        BindFuture { addr }
    }

    pub fn async_accept(self: &Arc<Self>) -> impl Future<Output = SimpleTcpStream> {
        struct AcceptFuture {
            listener: Arc<SimpleTcpListener>,
            first: bool,
        }

        impl Future for AcceptFuture {
            type Output = SimpleTcpStream;

            fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
                let mut listener = Arc::get_mut(&mut self.listener).unwrap();
                if let Ok((stream, _)) = listener.io.accept() {
                    return Poll::Ready(SimpleTcpStream { io: stream });
                }
                if self.first {
                    listener.register(cx);
                    self.first = false;
                }
                Poll::Pending
            }
        }

        AcceptFuture {
            listener: Arc::clone(self),
            first: false,
        }
    }
}
