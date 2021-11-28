ws_bridge_rs
============

Simple command line utility to forward a tcp connection over a websocket and vice versa. It's a Rust
rewrite of my Python [ws_bridge](https://github.com/iwanders/ws_bridge_py) package using
[tokio.rs](https://tokio.rs/) and `async` Rust.

Usage
-----

Dummy example to just forward a tcp connection:

```
# Build a release version that's optimised:
cargo build --release

# Start a tcp server on 127.0.0.1:3000, connect with a websocket to 127.0.0.1:3001
./target/release/ws_bridge_rs -vvv tcp_to_ws 127.0.0.1:3000 127.0.0.1:3001

# In another terminal, start a websocket server on 127.0.0.1:3001, connect with tcp to 127.0.0.1:3002
./target/release/ws_bridge_rs -vvv ws_to_tcp 127.0.0.1:3001 127.0.0.1:3002

# Now, the bridge is setup, tcp connections on 127.0.0.1:3000 will get forwarded to 127.0.0.1:3002.

# This can be tested by running a tcp server on port 3002;
nc -l 127.0.0.1 3002

# Then connect to port 3000 with netcat and communicate to the other netcat instance.
nc 127.0.0.1 3000
```

Performance
-----------
Benchmark using `iperf` and the above two `ws_bridge_rs` instances.

Server ran as `iperf -s -p 3002`, client with `iperf -p 3000 -c 127.0.0.1`;
```
Client connecting to 127.0.0.1, TCP port 3000
TCP window size: 2.50 MByte (default)
------------------------------------------------------------
[  3] local 127.0.0.1 port 42454 connected with 127.0.0.1 port 3000
[ ID] Interval       Transfer     Bandwidth
[  3]  0.0-10.0 sec  6.75 GBytes  5.80 Gbits/sec
```

The Python version reaches 550 mbit/s, the non-optimised rust version reaches 160 mbit/s.


License
------
All code licensed under MIT OR Apache-2.0.
