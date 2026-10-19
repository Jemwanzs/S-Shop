# 39. Business workspace addresses (roadmap 145)

Every business has its own address in S'Shop: **`https://s-shop.store/{business-slug}`**. All signed-in screens live
under it, for example `https://s-shop.store/pablo-niche/products?status=active&page=2`.

## The slug

- Generated once, when the business is created (approved access request, platform owner, bootstrap), from the
  business name: lowercase letters, numbers and dashes, 3–60 characters. It is unique across all businesses, present
  and past.
- The same slug names the customer ordering link (`/order/{slug}`), the business sign-in page (`/login/{slug}`) and the
  business website (`/s/{slug}`). None of those public addresses changed.
- **Permanent.** Renaming the business never changes it.
- **Reserved words** can never be a slug: the app's own first path segments (`login`, `order`, `track`, `r`, `s`, `api`,
  `products`, `sales`, `settings`… every workspace screen) plus `admin`, `platform`, `www`, `app`, `support`, `assets`,
  `help` and similar. A new business whose name would produce one gets `{name}-shop`.

## Changing it (controlled)

Settings → Business profile → *Business address* → **Change**. The field is locked until then. Saving asks for
confirmation and shows the new workspace and ordering addresses.

- Needs `settings.business` and a full sign-in (a Quick PIN session is asked to confirm the full PIN first).
- The new slug must be valid, not reserved, and not used now or before by another business.
- The old slug is kept (`tenant_slug_history`, migration 0041) and is never given to another business. Old workspace
  bookmarks, ordering links, sign-in pages, logos and websites keep working and lead to the business's current
  address. A business may take back one of its own old slugs.
- Audited as `settings / change_address` with the slug before and after.

## How the workspace uses it

- After sign-in (password, Quick PIN or Face ID), the app opens `/{slug}/…` — the chosen landing page, or the address
  someone opened before signing in (a bookmark or shared link). Several branches: the branch is chosen first, then the
  same address opens.
- Every in-app link, Back/Forward, refresh and direct link keep the slug. The catalogue's filters (roadmap 143) live in
  the same address.
- **Old addresses** without a slug (`/products`, `/settings/business`) redirect to `/{slug}/products` etc.
- **Switching business** (several businesses of one tenant, platform support sessions) moves to the other business's
  address, with that business's own cache and filters.

## Security

The address is **never** proof of access. The server decides the business from the session token on every request;
no API takes the slug for staff data. In the app:

| Address opened | Result |
|---|---|
| `/{own slug}/…` | the screen (role and branch rules as always) |
| `/{an old slug of this business}/…` | redirected to the current slug |
| `/{another business of the same tenant}/…` | a *Switch business?* prompt — never switched silently |
| `/{any other slug}/…` | back to the person's own workspace with a notice; nothing about that business is shown |
| any workspace address while signed out | the business sign-in page (`/login/{slug}`), then back to the address |

The Business profile shows the **S'Shop Workspace URL** with *Copy link*, for bookmarks and for staff.
