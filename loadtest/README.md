# SFU load test

N bots sign up through the real HTTP API, join one server, open the real websocket,
`JoinCall` its channel, publish one synthetic Opus track (50 pps, 100 B) and one
synthetic VP8 track (30 fps, 1150 B, one RTP packet per frame) and subscribe to
everyone else. Each bot expects `2 * (N - 1)` remote tracks.

## Run

1. Postgres with the harmony schema. (`backend/init-db/schema.sql` currently fails on a
   fresh Postgres: two foreign keys reference `messages` before it is created.)
2. Backend, pointed at that database:

   ```sh
   cd backend
   ROCKET_PORT=5500 ROCKET_DATABASES='{harmony_db={url="postgresql://..."}}' \
     SQLX_OFFLINE=true cargo run --release
   ```

3. Load test:

   ```sh
   cd loadtest
   cargo run --release -- --n 50 --duration-secs 60   # --api / --ws default to localhost:5500
   ```

It prints a markdown summary and writes the full report (per-bot numbers, packets per
second timeline) to `results/<unix time>-n<N>.json`.

- **join**: `JoinCall` sent until the bot's peer connection is connected.
- **received/s**: from the second after every bot had every track (or the second half).
- **loss**: gaps in the RTP sequence numbers of received tracks. Packets still queued
  when the run ends are not counted.
- **latency**: one-way, publisher to subscriber, for packets received in the second half
  of the run. Bots share one process, so the clocks are the same.

## Results

Apple M2 (8 cores), 24 GB RAM, macOS 14.4.1 (Darwin 23.4.0 arm64). The backend and
all bots ran on this machine over loopback. Backend: release build, single UDP port
(50000) for every peer connection. Spotlight indexing was also using CPU during the 50-bot run.

### 50 participants, 60 s

| metric | value |
|---|---|
| bots connected | 50/50 |
| join ms p50 / p95 / max | 1293 / 1444 / 1446 |
| bots with all 98 remote tracks | 50/50 (last one at 16.4 s) |
| RTP packets published/s (all bots) | 4001 (expected 4000) |
| RTP packets received/s, steady | 43134 (expected 196001) |
| loss from sequence gaps | 7.04% |
| one-way latency ms p50 / p95 / p99 / max | 25992 / 40193 / 42524 / 44754 |
| backend CPU (cputime per second) peak / mean | 186% / 147% |
| backend RSS peak | 599 MB |
| load test CPU peak / RSS peak | 198% / 449 MB |

Six 50-bot runs on the same code gave 33.5k–55.1k received packets/s with 5.1–19.5% loss.
50 participants, all sending video, overload the SFU on this machine. Once it falls behind,
packets wait in webrtc-rs's 1 MB-per-track SRTP read buffers, so latency keeps growing
instead of the packets being dropped.

### Where it saturates (30 s runs; N=5 is 20 s)

| N | expected pkts/s | received pkts/s | loss | latency p50 / p95 ms |
|---|---|---|---|---|
| 5 | 1600 | 1600 | 0.00% | 1 / 2 |
| 20 | 30400 | 30400 | 0.00% | 2 / 7 |
| 25 | 48000 | 48000 | 0.00% | 3 / 12 |
| 30 | 69600 | 69505 | 0.00% | 14 / 145 |
| 35 | 95200 | 69770 | 0.00% | 3898 / ≥9999 |
| 40 | 124800 | 59220 | 1.11% | 7003 / ≥9999 |
| 45 | 158401 | 50191 | 3.94% | ≥9999 / ≥9999 |

(These runs capped latency at 10 s. The 50-bot run above has no cap.)

On this machine the SFU forwards about 70k packets/s. Past that, throughput falls. A
profile (`sample`) of the overloaded backend showed its worker threads spending most of
their time in `sendto`. Moving the AES and GHASH work to ARMv8 instructions
(`--cfg aes_armv8 --cfg polyval_armv8`) took the crypto out of the profile but did not raise
throughput. With the single UDP port, the shared socket's receive queue was seen at up
to 372 KB of its 768 KB and the kernel's "full socket buffers" drop counter kept rising,
which points to it as the source of the loss. With one port per peer connection (an
experiment, not kept), three 50-bot runs had 0.01–1.29% loss and 73k–83k packets/s.
