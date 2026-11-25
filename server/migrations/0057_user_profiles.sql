-- Roadmap 185: each person's own profile — a profile photo (square, cropped in the browser, checked on the server) on
-- their sign-in identity, shared by their linked access to other businesses of the tenant (roadmap 72). Served by an
-- unguessable key that changes with every new photo, so it can be cached for good.
ALTER TABLE users ADD COLUMN avatar      bytea,
                  ADD COLUMN avatar_mime text,
                  ADD COLUMN avatar_key  uuid;
CREATE UNIQUE INDEX users_avatar_key_idx ON users (avatar_key) WHERE avatar_key IS NOT NULL;

-- The photo address for any user row (a linked row shows its identity's photo); NULL = initials.
CREATE FUNCTION user_avatar_url(uid uuid) RETURNS text LANGUAGE sql STABLE AS $$
    SELECT '/api/avatars/' || i.avatar_key FROM users u JOIN users i ON i.id = COALESCE(u.login_user_id, u.id)
    WHERE u.id = uid AND i.avatar_key IS NOT NULL
$$;

-- The audit trail keeps the name the person had when they acted (a later rename never rewrites history).
ALTER TABLE audit_log ADD COLUMN actor_name text;
