-- cursor pagination: WHERE channel_id = $1 AND message_id < $2 ORDER BY message_id DESC LIMIT n
CREATE INDEX IF NOT EXISTS idx_messages_channel_message ON messages (channel_id, message_id);

-- kept: users_routes.rs updates messages.s3_icon_key by user_id on every icon change
CREATE INDEX IF NOT EXISTS idx_user_id_hash ON messages USING hash (user_id);

-- redundant: hash indexes on primary keys and a duplicate of the users_servers PK
DROP INDEX IF EXISTS idx_server_id_hash;
DROP INDEX IF EXISTS idx_channel_id_hash;
DROP INDEX IF EXISTS idx_user_server_btree;
