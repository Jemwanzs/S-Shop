-- Roadmap 74–75: the tenant chooses whether its own domain is the website's main address (the S'Shop address then
-- redirects to it), and product photos get a small thumbnail for fast product grids (made in the browser at upload,
-- like website media; older photos keep loading at their optimised full size).
ALTER TABLE website_domains ADD COLUMN is_primary boolean NOT NULL DEFAULT true;
ALTER TABLE product_photos ADD COLUMN thumb bytea, ADD COLUMN thumb_mime text;
