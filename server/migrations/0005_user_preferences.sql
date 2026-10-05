-- Roadmap 8/9: per-user preferences (language, font, display currency). Typed defaults live in
-- routes/prefs.rs, so an empty object means "all defaults".
ALTER TABLE users ADD COLUMN preferences jsonb NOT NULL DEFAULT '{}'::jsonb;
