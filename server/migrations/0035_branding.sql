-- Roadmap 112: the business logo carries a fingerprint so browsers re-check it cheaply (304) and a new logo shows at
-- once instead of a cached old one.
ALTER TABLE tenants ADD COLUMN logo_etag text;
UPDATE tenants SET logo_etag = md5(logo) WHERE logo IS NOT NULL;
