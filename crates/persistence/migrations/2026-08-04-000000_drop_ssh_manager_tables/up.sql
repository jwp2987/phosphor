-- Drop the SSH-manager tables and the SSH gist-sync metadata table. The
-- fork-original SSH Manager feature (warp_ssh_manager + zap_sync) has been
-- removed, so these tables are permanently unused.
--
-- `ssh_servers` references both `ssh_nodes` and `ssh_onekey_credentials` via
-- foreign keys, so the child table must be dropped first.
DROP TABLE IF EXISTS ssh_servers;
DROP TABLE IF EXISTS ssh_onekey_credentials;
DROP TABLE IF EXISTS ssh_nodes;
DROP TABLE IF EXISTS sync_meta;
