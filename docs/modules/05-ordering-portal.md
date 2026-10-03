# 05 · Customer ordering portal

**Scope:** §16–18, §20–21 · **Public routes:** `/order/<business-slug>`, `/track/<token>` · **API:** `/api/portal…`

Customers need **no account**. The portal is image-rich, mobile-first and grows to a 3–5 column catalogue on desktops.
The business's logo, name and tagline lead every screen; a subtle “Powered by S'Shop” sits at the bottom.

## Flow
1. **Welcome** — “Enter the mobile number you used to order with us before (or will use this time)” → *Continue*.
2. **Verification (optional)** — with *Verify customers with a WhatsApp code* on (and WhatsApp configured) a 6-digit
   code is sent; 3 codes per 10 minutes, 5 attempts per code, 10-minute expiry. With verification on, names are not
   revealed before the code is entered.
3. **New customers** give a first name (+ optional nickname); the customer joins the Customer Book.
4. **Home** — “Welcome back, Pablo 👋”, loyalty card (points and value, if enabled), **My Orders** (“You've ordered
   N times with us”), **Order Now**, “Not you? Enter a different number”.
5. **Catalogue** — search, category chips, photo cards (name, price, In stock / Out of stock). Product sheet: main photo,
   gallery, description, availability, quantity, *Add to Cart*. A floating circular **🛒 N** button opens the cart.
6. **Checkout** — items with quantity controls, total, customer name & mobile, delivery location (required), notes →
   **Submit Order** → “Order submitted successfully. We are now processing your order.” → **Track My Order**.
7. **My Orders** — total orders and the latest 3; the active one shows the progress tracker.
8. **Tracking** (`/track/<token>`) — Order Received ✓ → Being prepared ✓ → On delivery ● → Delivered → Completed,
   delivering-to, items and total; “Reload this page any time for the latest status”. The token is unguessable.

## Rules
- Only active products marked *Available on ordering link* and sold at the fulfilling branch appear; quantities are
  limited to available stock (on hand − reserved). Out-of-stock items can be hidden (Settings → Orders).
- Orders are fulfilled by the configured branch (default: first active branch).
- The session lasts 30 days on that device and is bound to the business.
- New orders notify staff with `orders.manage` instantly; the customer gets a WhatsApp confirmation when configured.

## Settings
Orders → ordering link open, fulfilling branch, reserve stock, sale stage, WhatsApp verification, show out-of-stock,
customer notifications. Loyalty → show points / value on the portal. Business profile → slug, logo, tagline.
