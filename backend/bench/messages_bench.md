# Messages benchmark

Machine: Apple M2, 24 GB RAM; Docker VM 8 CPUs / 7.65 GiB; `postgres:15.2-alpine` with default config.
Driver: `pgbench -n -M prepared` run **inside** the container over the unix socket, so the latency here
leaves out any network time. Over a real network the old 5-round-trip send path loses by more than shown.

## Data

`HEAD:backend/init-db/schema.sql` (FK ordering fixed so it loads), then:

```sql
SELECT setseed(0.42);
INSERT INTO users (email, username, display_username, hashed_password)
SELECT 'u'||g||'@x.io', 'user'||g, 'User '||g, 'x' FROM generate_series(1,1000) g;
INSERT INTO servers (server_type, members, server_name)
SELECT 'server', 10, 'server '||g FROM generate_series(1,500) g;
INSERT INTO users_servers (user_id, server_id)
SELECT u, ((u + k) % 500) + 1 FROM generate_series(1,1000) u, generate_series(0,9) k ON CONFLICT DO NOTHING;
INSERT INTO channels (server_id) SELECT ((g - 1) % 500) + 1 FROM generate_series(1,1000) g;
INSERT INTO messages (user_id, channel_id, message, display_username, timestamp)
SELECT (random()*999)::int + 1, (random()*999)::int + 1, md5(g::text) || ' hello world message body', 'User',
       now() - (1000000 - g) * interval '1 second'
FROM generate_series(1,1000000) g;
VACUUM ANALYZE;
```

That gives 1,000,000 messages over 1,000 channels (467 to 1108 per channel, about 1000 on average), interleaved in the heap.
After the old-schema runs, `migrations/20261116000000_messages_pagination_index.sql` was applied (twice, to check
it is idempotent) and the tables were ANALYZEd again.

## Read: GET /api/messages/get/<channel_id>

```sql
-- old: whole channel
SELECT * FROM messages WHERE channel_id = :ch ORDER BY message_id ASC;
-- new: one page (before = i64::MAX for the newest page)
SELECT * FROM (SELECT * FROM messages WHERE channel_id = :ch AND message_id < :before
  ORDER BY message_id DESC LIMIT 50) page ORDER BY message_id ASC;
```

| query | ch 8 (969 rows) | ch 500 (988) | ch 999 (1037) |
|---|---|---|---|
| old, old schema (parallel seq scan, pgbench -t 20) | 20.700 ms | 18.902 ms | 19.703 ms |
| new, newest page (pgbench -t 200) | 0.146 ms | 0.108 ms | 0.113 ms |
| new, deep page `before=500000` (-t 200) | 0.121 ms | 0.112 ms | 0.112 ms |
| old full fetch, with the new index (-t 20) | 1.017 ms | 1.038 ms | 1.062 ms |

Channel 7 under the old query: 18.986 ms (985 rows; it was then used for the send runs).

EXPLAIN (ANALYZE, BUFFERS) for channel 500:
- old: Parallel Seq Scan + Sort, `Buffers: shared hit=15459`, Execution Time 25.515 ms
- new, newest page: Index Scan Backward using idx_messages_channel_message, `shared hit=47 read=3`, 0.081 ms
- new, `before=500000`: same plan, `shared hit=53 read=1`, 0.086 ms

## Send: POST /api/messages/send (10,000 sends, user 6 to channel 7, 1 client)

old = 5 autocommit statements as the old handler ran them (channel→server lookup, membership, INSERT,
UPDATE last_message_at, UPDATE last_message_id). new = the single CTE in `message_routes.rs` (SEND_MESSAGE_SQL).

| path | schema | latency avg | tps |
|---|---|---|---|
| old 5 statements | old indexes | 0.513 ms | 1950.6 |
| old 5 statements | new indexes | 0.480 ms | 2085.1 |
| new 1 statement | new indexes | 0.228 ms (second run 0.215 ms) | 4392.9 |

Note: the "new indexes" send rows were measured with `idx_user_id_hash` dropped. The final migration keeps it
(icon changes update messages by user_id), so inserts maintain one more index than in those rows; not re-measured.

Checked on the same DB: a non-member (user 6, channel 300) gets 0 rows and the message count stays the same.
A member gets one row with display_username taken from `users`, and servers.last_message_id / last_message_at
equal max(message_id) / max(timestamp).
