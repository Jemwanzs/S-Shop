-- A retried or repeated photo upload (same pending photo sent twice) stores the photo once.
ALTER TABLE product_photos ADD COLUMN upload_ref uuid;
CREATE UNIQUE INDEX product_photos_upload_ref_uq ON product_photos (product_id, upload_ref) WHERE upload_ref IS NOT NULL;
