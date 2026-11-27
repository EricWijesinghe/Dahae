-- The install's time zone for SQL date math. Each API connection carries `install.tz` as the `dahae.tz` startup
-- option; a connection without it (a psql session, a test pool) reads the default zone, so every
-- install from before time zones behaves exactly as it did. Additive: the previous release never calls it.
CREATE OR REPLACE FUNCTION dahae_tz() RETURNS text
    LANGUAGE sql STABLE PARALLEL SAFE
    AS $$ SELECT COALESCE(NULLIF(current_setting('dahae.tz', true), ''), 'Asia/Seoul') $$
