# rust-async-demo

some demo code for the report about rust async.

* mutli-client: create three threads which connect to the server to test if the server can handle multiple request concurrently.
* web-echo: a server implemented with mutli-thread.
* web-echo-mio: a server implemented with non-block crate mio.
* web-async: a server implemented with rust async.