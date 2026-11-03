-- Drop the server-side A/B experiment cache. The client no longer reads or
-- writes it (`ServerExperiment` and `ModelEvent::SaveExperiments` were removed),
-- so the table is permanently unused.
--
-- IF EXISTS keeps this safe on a database that never had the table.
DROP TABLE IF EXISTS server_experiments;
