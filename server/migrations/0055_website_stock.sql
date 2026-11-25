-- Roadmap 181: the website and ordering link show in-stock products only by default (the owner's specification). The one
-- existing setting (*Show out-of-stock products*, now presented as *Show in-stock products only*) is switched to it for
-- every business; a business can switch it back. Products, publication and stock are never changed by it.
UPDATE tenants SET settings = jsonb_set(settings, '{orders,show_out_of_stock}', 'false'::jsonb, false)
 WHERE settings #> '{orders,show_out_of_stock}' = 'true'::jsonb;
