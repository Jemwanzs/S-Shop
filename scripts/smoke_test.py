#!/usr/bin/env python3
"""End-to-end smoke test for a running S'Shop server (standard library only).

Usage:
    python scripts/smoke_test.py http://localhost:8080 admin@example.com 1234

Creates test data in the target business — run it against a scratch database,
never production.
"""
import json
import sys
import time
import urllib.error
import urllib.request
import uuid

BASE, EMAIL, PIN = (sys.argv[1:4] + [None] * 3)[:3]
if not (BASE and EMAIL and PIN):
    sys.exit(__doc__)

TOKEN = None
BRANCH = None
FAILURES = []


def call(method, path, body=None, expect=200, token=None, branch=None, location=None):
    req = urllib.request.Request(BASE + "/api" + path.replace(" ", "%20"), method=method)
    req.add_header("Content-Type", "application/json")
    if token or TOKEN:
        req.add_header("Authorization", "Bearer " + (token or TOKEN))
    if branch or BRANCH:
        req.add_header("X-Branch-Id", branch or BRANCH)
    if location:
        req.add_header("X-Location", location)
    data = json.dumps(body).encode() if body is not None else None
    try:
        with urllib.request.urlopen(req, data) as r:
            status, raw = r.status, r.read()
    except urllib.error.HTTPError as e:
        status, raw = e.code, e.read()
    payload = json.loads(raw) if raw and raw[:1] in (b"{", b"[") else raw
    if status != expect:
        raise AssertionError(f"{method} {path} → {status} (expected {expect}): {payload}")
    return payload


def check(name, cond, detail=""):
    print(("  ✓ " if cond else "  ✗ ") + name + (f" — {detail}" if detail and not cond else ""))
    if not cond:
        FAILURES.append(name)


def step(title):
    print(f"\n▸ {title}")


step("Sign in")
login = call("POST", "/auth/login", {"email": EMAIL, "pin": PIN})
TOKEN = login["token"]
BRANCH = login["profile"]["branches"][0]["id"]
# A run that crashed part-way may have left trading-hour or location rules on: start from the defaults.
_cfg = call("GET", "/settings")["settings"]
if _cfg["workspace"]["location"]["mode"] != "anywhere" or _cfg["workspace"]["outside_hours"] != "allow" or _cfg["workspace"]["hours"]["open"] != _cfg["workspace"]["hours"]["close"]:
    _cfg["workspace"] = {"hours": {"days": [True] * 7, "open": "00:00", "close": "00:00"}, "outside_hours": "allow", "location": {**_cfg["workspace"]["location"], "mode": "anywhere"}}
    call("PUT", "/settings", _cfg)
check("admin has full access", "*" in login["profile"]["permissions"])
call("POST", "/auth/login", {"email": EMAIL, "pin": "0000"}, expect=400)
check("wrong PIN rejected", True)

step("Second branch + product catalogue")
suffix = uuid.uuid4().hex[:4].upper()
b2 = call("POST", "/branches", {"name": f"Westlands {suffix}", "code": f"WL{suffix}"})["id"]
cat = call("POST", "/categories", {"name": f"Vegetables {suffix}"})["id"]
nduma = call("POST", "/products", {"name": f"Nduma {suffix}", "marked_price": 400, "max_discount": 50, "cost_price": 250,
                                   "category_id": cat})["result"]["id"]
watch = call("POST", "/products", {"name": f"Classic Watch {suffix}", "marked_price": 5000, "cost_price": 3000, "track_items": True,
                                   "loyalty_threshold": 1000, "loyalty_points_per": 2})["result"]["id"]
check("products created", bool(nduma and watch))

step("Receive stock (ledger)")
r = call("POST", "/stock/receive", {"product_id": nduma, "quantity": 20, "cost_price": 250})
check("quantity stock received", r["result"]["on_hand"] == 20, r)
codes = [f"W{suffix}{i}" for i in range(3)]
r = call("POST", "/stock/receive", {"product_id": watch, "quantity": 3, "barcodes": codes})
check("tracked items received", r["result"]["on_hand"] == 3, r)
call("POST", "/stock/receive", {"product_id": watch, "quantity": 1, "barcodes": [codes[0]]}, expect=422)
check("duplicate active barcode rejected", True)

step("Counter sale: cash, new customer, barcode-cleared item")
mobile = "07" + str(int(time.time()))[-8:]
sale = call("POST", "/sales", {
    "customer": {"mobile": mobile, "first_name": "Libbie"},
    "items": [{"product_id": nduma, "quantity": 2, "unit_price": 380},
              {"product_id": watch, "quantity": 1, "unit_price": 5000, "barcode": codes[0]}],
    "payment": {"method": "cash"},
    "client_ref": str(uuid.uuid4()),
})
s = sale["sale"]
check("total = 2×380 + 5000", float(s["total"]) == 5760, s["total"])
check("discount = 2×20", float(s["discount_total"]) == 40, s["discount_total"])
check("points: 760/500→1 + 5000/1000×2→10", s["points_earned"] == 11, s["points_earned"])
cust_id = s["customer"]["id"]
call("POST", "/sales", {"items": [{"product_id": watch, "quantity": 1, "unit_price": 5000, "barcode": codes[0]}],
                        "payment": {"method": "cash"}}, expect=422)
check("sold barcode cannot be sold again", True)
levels = call("GET", f"/stock?q=Nduma {suffix}")["items"][0]
check("stock deducted 20→18", levels["on_hand"] == 18, levels)

step("Idempotent retry")
ref = str(uuid.uuid4())
a = call("POST", "/sales", {"items": [{"product_id": nduma, "quantity": 1, "unit_price": 400}], "payment": {"method": "cash"}, "client_ref": ref})
b = call("POST", "/sales", {"items": [{"product_id": nduma, "quantity": 1, "unit_price": 400}], "payment": {"method": "cash"}, "client_ref": ref})
check("same client_ref returns the same sale", a["sale"]["id"] == b["sale"]["id"])

step("Credit sale + partial repayment")
cs = call("POST", "/sales", {"customer_id": cust_id, "items": [{"product_id": nduma, "quantity": 2, "unit_price": 400}],
                             "payment": {"method": "credit"}})
credit_id = cs["credit"]["id"]
check("credit record created", float(cs["credit"]["balance"]) == 800, cs["credit"])
r = call("POST", f"/credit/{credit_id}/payments", {"amount": 300, "method": "cash"})
check("balance after repayment", float(r["balance"]) == 500 and r["status"] == "partially_paid", r)
call("POST", f"/credit/{credit_id}/payments", {"amount": 900, "method": "cash"}, expect=422)
check("over-payment rejected", True)

step("Return with stock restoration + points reversal")
item_id = next(i for i in sale["items"] if i["product_id"] == nduma)["id"]
r = call("POST", f"/sales/{s['id']}/return", {"items": [{"sale_item_id": item_id, "quantity": 1}], "reason": "Damaged", "restock": True})
check("partial return processed", r["result"]["status"] == "partially_returned", r)
levels = call("GET", f"/stock?q=Nduma {suffix}")["items"][0]
check("returned unit back in stock (20−2−1−2+1)", levels["on_hand"] == 16, levels["on_hand"])

step("Customer ordering portal")
slug = login["profile"]["tenant"]["slug"]
call("GET", f"/portal/{slug}", token="none")
pmobile = "07" + str(int(time.time() * 7))[-8:]
ident = call("POST", f"/portal/{slug}/identify", {"mobile": pmobile})
check("new portal customer detected", ident["exists"] is False)
sess = call("POST", f"/portal/{slug}/session", {"mobile": pmobile, "first_name": "Evans"})
ptoken = sess["token"]
cat_items = call("GET", f"/portal/{slug}/catalogue", token=ptoken)["products"]
check("catalogue lists in-stock product", any(p["id"] == nduma for p in cat_items))
order = call("POST", f"/portal/{slug}/orders", {"items": [{"product_id": nduma, "quantity": 3}], "delivery_location": "GreenSpan Area"}, token=ptoken)
check("order number format", order["order_no"].startswith("ORD-"), order)
mine = call("GET", f"/portal/{slug}/orders", token=ptoken)
check("My Orders shows the order", mine["total_orders"] == 1)

step("Order fulfilment: reservation → sale")
before = call("GET", f"/stock?q=Nduma {suffix}")["items"][0]
call("POST", f"/orders/{order['id']}/status", {"status": "confirmed"})
after = call("GET", f"/stock?q=Nduma {suffix}")["items"][0]
check("confirmed order reserves 3", after["reserved"] == before["reserved"] + 3 and after["available"] == before["available"] - 3, after)
call("POST", f"/orders/{order['id']}/status", {"status": "delivered"}, expect=422)
check("delivery requires payment details", True)
done = call("POST", f"/orders/{order['id']}/status", {"status": "delivered", "payment": {"method": "cash"}})
check("delivered order became a sale", done["sale_id"] is not None)
final = call("GET", f"/stock?q=Nduma {suffix}")["items"][0]
check("reservation released and stock cleared", final["reserved"] == before["reserved"] and final["on_hand"] == before["on_hand"] - 3, final)
track = call("GET", f"/portal/track/{order['track_token']}", token="none")
check("public tracking shows delivered", track["order"]["status"] == "delivered")

step("Transfer: dispatch → in transit → receive")
t = call("POST", "/transfers", {"to_branch_id": b2, "items": [{"product_id": nduma, "quantity": 2}], "submit": True})
check("transfer approved (no workflow)", t["status"] == "approved", t)
call("POST", f"/transfers/{t['id']}/dispatch")
dest = call("GET", f"/stock?q=Nduma {suffix}", branch=b2)["items"][0]
check("in transit: not yet sellable at destination", dest["on_hand"] == 0, dest)
call("POST", f"/transfers/{t['id']}/receive", branch=b2)
dest = call("GET", f"/stock?q=Nduma {suffix}", branch=b2)["items"][0]
check("received at destination", dest["on_hand"] == 2, dest)

step("Maker-checker (roadmap 2): two-level chain")
roles = call("GET", "/roles")
mgr_role = next(r["id"] for r in roles if r["name"] == "Manager")
call("PUT", "/settings/workflows/stock.write_off",
     {"enabled": True, "levels": [{"approver_type": "role", "approver_role_id": mgr_role}, {"approver_type": "admin"}], "min_amount": None})
users = {}
for who in ("clerk", "mgr2"):
    mail = f"{who}{suffix.lower()}@sshop.test"
    call("POST", "/users", {"name": who.title(), "email": mail, "pin": "4321", "role_id": mgr_role, "all_branches": False, "branch_ids": [BRANCH]})
    users[who] = call("POST", "/auth/login", {"email": mail, "pin": "4321"})["token"]
clerk, mgr2 = users["clerk"], users["mgr2"]
w = call("POST", "/stock/adjustments", {"product_id": nduma, "kind": "write_off", "quantity": 1, "reason": "Rotten"}, token=clerk)
check("write-off parked for approval", w["pending_approval"] is True, w)
call("POST", f"/approvals/{w['approval_id']}/approve", {"comments": "self"}, token=clerk, expect=403)
check("requester cannot approve own request", True)
lvl = call("POST", f"/approvals/{w['approval_id']}/approve", {"comments": "level 1 ok"}, token=mgr2)
check("level 1 approval passes to level 2", lvl.get("status") == "pending" and lvl.get("level") == 2, lvl)
pending = next(a for a in call("GET", "/approvals?status=pending")["items"] if a["id"] == w["approval_id"])
check("request shows level 2 of 2 with decision trail", pending["level"] == 2 and pending["levels"] == 2 and len(pending["decisions"]) == 1, pending)
call("POST", f"/approvals/{w['approval_id']}/approve", {"comments": "again"}, token=mgr2, expect=403)
check("same person cannot approve two levels", True)
def adj_status(): return next(a["status"] for a in call("GET", "/stock/adjustments?period=today")["items"] if a["id"] == pending["entity_id"])
check("not applied before the last level", adj_status() == "pending", adj_status())
call("POST", f"/approvals/{w['approval_id']}/approve", {"comments": "final"})
check("applied after the last level", adj_status() == "applied", adj_status())
call("PUT", "/settings/workflows/stock.write_off", {"enabled": False, "levels": [{"approver_type": "admin"}], "min_amount": None})

step("Maker-checker (roadmap 2): conditional expense rule")
cats = call("GET", "/expense-categories")
rent, transport = next(c["id"] for c in cats if c["name"] == "Rent"), next(c["id"] for c in cats if c["name"] == "Transport")
call("PUT", "/settings/workflows/expense",
     {"enabled": True, "levels": [{"approver_type": "admin"}], "min_amount": 1000, "conditions": {"category_ids": [rent]}})
e1 = call("POST", "/expenses", {"category_id": transport, "amount": 5000, "description": "Fuel"}, token=clerk)
check("other category is not gated", e1["pending_approval"] is False, e1)
e2 = call("POST", "/expenses", {"category_id": rent, "amount": 500, "description": "Deposit"}, token=clerk)
check("below threshold is not gated", e2["pending_approval"] is False, e2)
e3 = call("POST", "/expenses", {"category_id": rent, "amount": 5000, "description": "Monthly rent"}, token=clerk)
check("matching category + amount needs approval", e3["pending_approval"] is True, e3)
call("PUT", "/settings/workflows/expense",
     {"enabled": True, "levels": [{"approver_type": "admin"}], "min_amount": None, "conditions": {"category_ids": [rent]}}, expect=200)
call("PUT", "/settings/workflows/stock.add",
     {"enabled": True, "levels": [{"approver_type": "admin"}], "min_amount": None, "conditions": {"category_ids": [rent]}}, expect=400)
check("category conditions only allowed for expenses", True)
call("PUT", "/settings/workflows/expense", {"enabled": False, "levels": [{"approver_type": "admin"}], "min_amount": None, "conditions": {}})

step("Ledger integrity: on_hand = Σ movements")
pos = call("GET", f"/stock/position?period=all&product_id={nduma}&branch_id={BRANCH}")["rows"][0]
lvl = call("GET", f"/stock?q=Nduma {suffix}")["items"][0]
check("position closing matches level", pos["closing"] == lvl["on_hand"], (pos["closing"], lvl["on_hand"]))

step("Roadmap 1a: custom product fields")
fid = call("POST", "/product-fields", {"label": f"Size {suffix}", "field_type": "dropdown", "options": ["S", "M", "L"], "required": True})["id"]
key = next(f["key"] for f in call("GET", "/product-fields") if f["id"] == fid)
call("POST", "/products", {"name": f"Shirt {suffix}", "marked_price": 900}, expect=400)
check("required product field enforced", True)
call("POST", "/products", {"name": f"Shirt {suffix}", "marked_price": 900, "custom_fields": {key: "XL"}}, expect=400)
check("dropdown value validated", True)
shirt = call("POST", "/products", {"name": f"Shirt {suffix}", "marked_price": 900, "custom_fields": {key: "M"}})["result"]["id"]
check("custom field value saved", call("GET", f"/products/{shirt}")["product"]["custom_fields"].get(key) == "M")
call("PUT", f"/product-fields/{fid}", {"label": f"Size {suffix}", "field_type": "dropdown", "options": ["S", "M", "L"], "required": False, "is_active": False})

step("Roadmap 1b: configurable order statuses")
settings = call("GET", "/settings")["settings"]
original = json.loads(json.dumps(settings))
for st in settings["orders"]["statuses"]:
    if st["key"] == "preparing":
        st["label"] = "In the kitchen"
    if st["key"] == "on_delivery":
        st["enabled"] = False
call("PUT", "/settings", settings)
o2 = call("POST", f"/portal/{slug}/orders", {"items": [{"product_id": nduma, "quantity": 1}], "delivery_location": "Shop"}, token=ptoken)
call("POST", f"/orders/{o2['id']}/status", {"status": "on_delivery"}, expect=422)
check("disabled step cannot be used", True)
call("POST", f"/orders/{o2['id']}/status", {"status": "preparing"})
tr = call("GET", f"/portal/track/{o2['track_token']}", token="none")
labels = [s["label"] for s in tr["steps"]]
check("renamed step shown to customers", tr["order"]["status_label"] == "In the kitchen" and "In the kitchen" in labels, labels)
check("disabled step hidden from tracker", all(s["status"] != "on_delivery" for s in tr["steps"]), labels)
bad = json.loads(json.dumps(settings))
bad["orders"]["statuses"].append({"key": "teleported", "label": "Teleported", "enabled": True})
call("PUT", "/settings", bad, expect=400)
check("unknown status rejected", True)
call("POST", f"/orders/{o2['id']}/status", {"status": "cancelled"})
call("PUT", "/settings", original)

step("Roadmap 3: threshold-based medals")
d = call("GET", "/dashboard?period=today")
check("rank mode: top seller is Gold", d["top_products_revenue"][0]["medal"] == "Gold", d["top_products_revenue"][:1])
settings = call("GET", "/settings")["settings"]
original = json.loads(json.dumps(settings))
top = float(d["top_products_revenue"][0]["revenue"])
settings["reports"]["medals"] = {"mode": "targets",
    "products": {"basis": "revenue", "gold": top * 10, "silver": top, "bronze": 0},
    "staff": {"basis": "units", "gold": 1, "silver": 0, "bronze": 0}}
call("PUT", "/settings", settings)
d = call("GET", "/dashboard?period=today")
p0 = d["top_products_revenue"][0]
check("targets mode: reaching the silver target earns Silver", p0["medal"] == "Silver", p0)
check("targets mode: below every target earns nothing",
      all(p["medal"] is None for p in d["top_products_revenue"] if float(p["revenue"]) < top), d["top_products_revenue"])
check("staff target by units", all(u["medal"] == "Gold" for u in d["by_user"] if u["units"] >= 1), d["by_user"])
w = call("GET", "/dashboard?period=week")
days = (__import__("datetime").date.fromisoformat(w["to"]) - __import__("datetime").date.fromisoformat(w["from"])).days + 1
if days > 1 and float(w["top_products_revenue"][0]["revenue"]) < top * days:
    check("targets scale with the period length", w["top_products_revenue"][0]["medal"] is None, w["top_products_revenue"][0])
bad = json.loads(json.dumps(settings))
bad["reports"]["medals"]["products"] = {"basis": "revenue", "gold": 100, "silver": 200, "bronze": 0}
call("PUT", "/settings", bad, expect=400)
check("targets must go down Gold > Silver > Bronze", True)
call("PUT", "/settings", original)

step("Roadmap 4a: deposit on a credit sale")
line = [{"product_id": nduma, "quantity": 1, "unit_price": 400}]
call("POST", "/sales", {"customer_id": cust_id, "items": line, "payment": {"method": "credit"},
                        "deposit": {"amount": 400, "method": "cash"}}, expect=422)
check("deposit equal to the total is refused", True)
call("POST", "/sales", {"customer_id": cust_id, "items": line, "payment": {"method": "credit"},
                        "deposit": {"amount": 100, "method": "mpesa", "reference": "BAD"}}, expect=422)
check("M-Pesa deposit needs a valid code", True)
ds = call("POST", "/sales", {"customer_id": cust_id, "items": line, "payment": {"method": "credit"},
                             "deposit": {"amount": 150, "method": "cash"}})
check("credit opens partially paid at the balance", ds["credit"]["status"] == "partially_paid" and float(ds["credit"]["balance"]) == 250, ds["credit"])
check("sale records the deposit as paid", float(ds["sale"]["amount_paid"]) == 150, ds["sale"]["amount_paid"])
check("deposit shown on the receipt payments", [float(x["amount"]) for x in ds["payments"]] == [150], ds["payments"])
cd = call("GET", f"/credit/{ds['credit']['id']}")
check("deposit in the credit payment history", [float(x["amount"]) for x in cd["payments"]] == [150], cd["payments"])
r = call("POST", f"/credit/{ds['credit']['id']}/payments", {"amount": 250, "method": "cash"})
check("remaining balance can be settled", r["status"] == "paid" and float(r["balance"]) == 0, r)
nd = call("POST", "/sales", {"customer_id": cust_id, "items": line, "payment": {"method": "credit"}})
check("credit without deposit unchanged", nd["credit"]["status"] == "outstanding" and float(nd["credit"]["balance"]) == 400, nd["credit"])

step("Roadmap 7: request access (no open signup)")
req = {"business_name": f"Duka {suffix}", "contact_name": "Achieng", "email": f"duka{suffix.lower()}@sshop.test", "phone": "0711 222 333",
       "location": "Kisumu", "business_type": "Retail shop", "branches": 2, "message": "Please onboard us"}
call("POST", "/access-requests", {**req, "email": "not-an-email"}, token="none", expect=400)
check("invalid email refused", True)
call("POST", "/access-requests", {**req, "email": EMAIL}, token="none", expect=422)
check("existing user email refused", True)
r = call("POST", "/access-requests", req, token="none")
check("request accepted with support numbers", r["ok"] and "0798 993 404" in r["support_phones"], r)
call("POST", "/access-requests", req, token="none")
call("POST", "/access-requests", {**req, "email": f"bot{suffix.lower()}@sshop.test", "website": "spam.example"}, token="none")
pending = call("GET", "/platform/access-requests?status=pending")["items"]
mine = [x for x in pending if x["business_name"] == req["business_name"]]
check("stored once for review (duplicate ignored)", len(mine) == 1, len(mine))
check("honeypot submission dropped", not any(x["email"].startswith("bot") for x in pending))
call("GET", "/platform/access-requests", token=clerk, expect=403)
check("non platform admin cannot review", True)
ap = call("POST", f"/platform/access-requests/{mine[0]['id']}/approve")
check("approval returns a one-time PIN", len(ap["temporary_pin"]) == 8 and ap["slug"].startswith("duka-"), ap)
t2 = call("POST", "/auth/login", {"email": req["email"], "pin": ap["temporary_pin"]})
me2 = t2["profile"]
check("new business admin can sign in", me2["tenant"]["name"] == req["business_name"] and me2["user"]["role"] == "Tenant Administrator", me2["tenant"])
check("new admin is not a platform admin", me2["user"]["platform_admin"] is False)
call("POST", f"/platform/access-requests/{mine[0]['id']}/approve", expect=422)
check("cannot approve twice", True)
rq2 = call("POST", "/access-requests", {**req, "email": f"later{suffix.lower()}@sshop.test"}, token="none")
other = next(x for x in call("GET", "/platform/access-requests?status=pending")["items"] if x["email"].startswith("later"))
call("POST", f"/platform/access-requests/{other['id']}/reject", {"note": "Not a fit yet"})
check("rejected request leaves pending list", all(x["id"] != other["id"] for x in call("GET", "/platform/access-requests?status=pending")["items"]))

step("Roadmap 8: user preferences & exchange rates")
call("PUT", "/auth/preferences", {"language": "en", "font": "Comic Sans", "currency": "KES"}, expect=400)
check("unknown font refused", True)
call("PUT", "/auth/preferences", {"language": "en", "font": "Poppins", "currency": "XYZ"}, expect=400)
check("unknown currency refused", True)
call("PUT", "/auth/preferences", {"language": "en", "font": "Poppins", "currency": "USD"})
me = call("GET", "/auth/me")
check("preferences saved on the profile", me["user"]["preferences"] == {"language": "en", "font": "Poppins", "currency": "USD"}, me["user"]["preferences"])
fx = call("GET", "/fx")
check("exchange rates for KES/USD/EUR", fx["base"] == "KES" and fx["rates"]["KES"] == 1 and 0 < fx["rates"]["USD"] < 1, fx)
call("PUT", "/auth/preferences", {"language": "de", "font": "Outfit", "currency": "KES"}, expect=400)
check("unsupported language refused", True)
for lang in ("sw", "fr", "ar"):
    call("PUT", "/auth/preferences", {"language": lang, "font": "Outfit", "currency": "KES"})
check("Swahili, French and Arabic accepted", call("GET", "/auth/me")["user"]["preferences"]["language"] == "ar")
call("PUT", "/auth/preferences", {"language": "en", "font": "Outfit", "currency": "KES"})

step("Barcode scanning: costs, branch availability, tracked orders")
store_role = next(r["id"] for r in call("GET", "/roles") if r["name"] == "Storekeeper")
skmail = f"store{suffix.lower()}@sshop.test"
call("POST", "/users", {"name": "Store", "email": skmail, "pin": "4321", "role_id": store_role, "all_branches": True, "branch_ids": []})
store = call("POST", "/auth/login", {"email": skmail, "pin": "4321"})["token"]
look = call("GET", f"/products/lookup?code={codes[1]}", token=store)
check("item barcode resolves to its product", look["product"]["id"] == watch and look["stock_item"]["status"] == "in_stock", look["stock_item"])
check("cost price hidden on lookup without financial access", look["product"]["cost_price"] is None, look["product"]["cost_price"])
plist = call("GET", f"/products?q=Nduma {suffix}", token=store)["items"]
check("cost price hidden on product list", plist and plist[0]["cost_price"] is None, plist[:1])
check("cost price hidden on product detail", call("GET", f"/products/{nduma}", token=store)["product"]["cost_price"] is None)
lv = call("GET", f"/stock?q=Nduma {suffix}", token=store)["items"][0]
check("cost and cost-based value hidden on stock levels", lv["cost_price"] is None and lv["value"] is None, lv)
mv = call("GET", "/stock/movements?period=today&limit=50", token=store)["items"]
check("unit cost hidden on movements", all(m["unit_cost"] is None for m in mv))
check("admin still sees the cost", float(call("GET", f"/products/{nduma}")["product"]["cost_price"]) == 250)
call("PUT", f"/products/{nduma}", {"name": f"Nduma {suffix}", "marked_price": 400, "max_discount": 50, "category_id": cat, "cost_price": None}, token=store)
check("saving without cost access keeps the cost", float(call("GET", f"/products/{nduma}")["product"]["cost_price"]) == 250)
call("GET", f"/products/lookup?code=NO-SUCH-{suffix}", expect=404)
check("unknown barcode returns not found", True)
other = call("GET", f"/products/lookup?code={codes[1]}", branch=b2)
check("lookup lists other branches with stock", any(o["branch_id"] != b2 and o["available"] >= 1 for o in other["other_branches"]), other["other_branches"])
check("item seen from another branch is not local", other["stock_item"]["in_current_branch"] is False)
# A customer order for a tracked product needs the unit barcodes when it becomes a sale.
worder = call("POST", f"/portal/{slug}/orders", {"items": [{"product_id": watch, "quantity": 1}], "delivery_location": "Shop"}, token=ptoken)
wdetail = call("GET", f"/orders/{worder['id']}")
check("order detail flags tracked items", wdetail["items"][0]["track_items"] is True)
call("POST", f"/orders/{worder['id']}/status", {"status": "delivered", "payment": {"method": "cash"}}, expect=422)
check("tracked order cannot complete without unit barcodes", True)
call("POST", f"/orders/{worder['id']}/status", {"status": "delivered", "payment": {"method": "cash"}, "barcodes": [{"product_id": watch, "barcodes": [codes[1], codes[2]]}]}, expect=422)
check("barcode count must match the quantity", True)
wdone = call("POST", f"/orders/{worder['id']}/status", {"status": "delivered", "payment": {"method": "cash"}, "barcodes": [{"product_id": watch, "barcodes": [codes[1]]}]})
check("tracked order completes with its unit barcode", wdone["sale_id"] is not None, wdone)
check("that unit is now sold", call("GET", f"/products/lookup?code={codes[1]}")["stock_item"]["status"] == "sold")

step("Platform: open another business (audited)")
plist = call("GET", "/platform/tenants")
other_t = next(x for x in plist["items"] if x["id"] == ap["tenant_id"])
check("platform lists businesses", bool(other_t) and plist["home_tenant_id"] == login["profile"]["tenant"]["id"])
call("GET", "/platform/tenants", token=clerk, expect=403)
check("staff cannot list businesses", True)
call("POST", f"/platform/tenants/{other_t['id']}/open", token=clerk, expect=403)
check("staff cannot open another business", True)
op = call("POST", f"/platform/tenants/{other_t['id']}/open")
check("opened business profile shows acting", op["profile"]["tenant"]["id"] == other_t["id"] and op["profile"]["acting"]["home_tenant_id"] == login["profile"]["tenant"]["id"], op["profile"].get("acting"))
acting = op["token"]
other_branch = op["profile"]["branches"][0]["id"]
me_act = call("GET", "/auth/me", token=acting, branch=other_branch)
check("full access inside the opened business", "*" in me_act["permissions"] and me_act["tenant"]["id"] == other_t["id"])
check("acting session sees only that business's data", all(x["id"] != nduma for x in call("GET", "/products?status=all", token=acting, branch=other_branch)["items"]))
audit_o = call("GET", "/audit?period=today&limit=200", token=acting, branch=other_branch)["items"]
check("opening is in that business's audit trail", any(x.get("action") == "open_business" for x in audit_o))
back = call("POST", f"/platform/tenants/{login['profile']['tenant']['id']}/open", token=acting, branch=other_branch)
check("return gives a normal session", back["profile"]["acting"] is None and back["profile"]["tenant"]["id"] == login["profile"]["tenant"]["id"])

step("Roadmap 14: own-data scoping, My Dashboard, activity, safe role management")
roles14 = {r["name"]: r for r in call("GET", "/roles")}
def make_user(tag, role_id):
    mail = f"{tag}{suffix.lower()}@sshop.test"
    call("POST", "/users", {"name": tag.title(), "email": mail, "pin": "4321", "role_id": role_id, "all_branches": True, "branch_ids": []})
    return call("POST", "/auth/login", {"email": mail, "pin": "4321"})["token"]
seller = make_user("seller", roles14["Salesperson"]["id"])
mine_sale = call("POST", "/sales", {"items": [{"product_id": nduma, "quantity": 1, "unit_price": 400}], "payment": {"method": "cash"}, "client_ref": str(uuid.uuid4())}, token=seller)
seller_list = call("GET", "/sales?period=all&limit=200", token=seller)["items"]
check("salesperson sees only own sales", seller_list and all(x["id"] == mine_sale["sale"]["id"] for x in seller_list), len(seller_list))
call("GET", f"/sales/{s['id']}", token=seller, expect=403)
check("salesperson cannot open a colleague's sale", True)
call("GET", f"/sales/{mine_sale['sale']['id']}", token=seller)
check("salesperson opens own sale", True)
admin_id = login["profile"]["user"]["id"]
md = call("GET", f"/dashboard?period=today&mine=true&user_id={admin_id}", token=seller)
check("My Dashboard ignores another user id", float(md["kpis"]["sales"]) == 400 and md["mine"] is True, md["kpis"]["sales"])
check("My Dashboard shows own rank only", md["my_rank"] is not None and md["by_user"] == [], md.get("my_rank"))
mgr_dash = call("GET", f"/dashboard?period=today&user_id={admin_id}")
check("manager can filter by another employee", mgr_dash["kpis"]["sales"] is not None)
act = call("GET", "/dashboard/activity?mine=true", token=seller)
check("activity feed shows own actions", act and all(x["user"] in ("Seller", None) for x in act), [x["title"] for x in act][:3])
check("business activity feed for managers", len(call("GET", "/dashboard/activity")) > 0)

repo_role = call("POST", "/roles", {"name": f"Reporter {suffix}", "permissions": ["reports.view", "sales.view"]})["id"]
reporter = make_user("reporter", repo_role)
cat_keys = [r["key"] for r in call("GET", "/reports", token=reporter)["reports"]]
check("per-employee reports hidden without permission", "user_performance" not in cat_keys and "sales" in cat_keys, cat_keys)
call("GET", "/reports/user_performance?period=today", token=reporter, expect=403)
check("per-employee report refused without permission", True)

lead_role = call("POST", "/roles", {"name": f"People lead {suffix}", "permissions": ["roles.manage", "users.manage", "sales.view"]})["id"]
lead = make_user("lead", lead_role)
call("POST", "/roles", {"name": f"Sneaky {suffix}", "permissions": ["settings.manage"]}, token=lead, expect=403)
check("cannot create a role with permissions you lack", True)
call("POST", "/users", {"name": "Promoted", "email": f"promo{suffix.lower()}@sshop.test", "pin": "4321", "role_id": roles14["Manager"]["id"], "all_branches": True, "branch_ids": []}, token=lead, expect=403)
check("cannot assign a role more powerful than yours", True)
call("PUT", f"/roles/{lead_role}", {"name": f"People lead {suffix}", "permissions": ["roles.manage", "users.manage", "sales.view", "approvals.approve"]}, token=lead, expect=403)
check("cannot add permissions to a role beyond your own", True)

stock_role = call("POST", "/roles", {"name": f"Stock settings {suffix}", "permissions": ["settings.stock"]})["id"]
stocker = make_user("stocker", stock_role)
cfg = call("GET", "/settings")["settings"]
c2 = json.loads(json.dumps(cfg)); c2["stock"]["low_stock_threshold"] = cfg["stock"]["low_stock_threshold"] + 1
call("PUT", "/settings", c2, token=stocker)
check("area permission allows its own settings", True)
c3 = json.loads(json.dumps(c2)); c3["loyalty"]["points_per"] = cfg["loyalty"]["points_per"] + 1
call("PUT", "/settings", c3, token=stocker, expect=403)
check("area permission cannot change other areas", True)
call("PUT", "/settings", cfg)

call("PUT", f"/roles/{repo_role}", {"name": f"Reporter {suffix}", "permissions": ["reports.view", "sales.view"], "is_active": False}, expect=422)
check("role with users cannot be retired", True)
spare = call("POST", "/roles", {"name": f"Spare {suffix}", "permissions": ["sales.view"]})["id"]
call("PUT", f"/roles/{spare}", {"name": f"Spare {suffix}", "permissions": ["sales.view"], "is_active": False})
call("POST", "/users", {"name": "Late", "email": f"late{suffix.lower()}@sshop.test", "pin": "4321", "role_id": spare, "all_branches": True, "branch_ids": []}, expect=400)
check("retired role cannot be assigned", True)

step("Roadmap 15: leaderboards")
for m in ["revenue", "units", "sales", "orders", "profit", "margin"]:
    lb = call("GET", f"/leaderboards/products?period=today&metric={m}")
    vals = [float(x[m]) for x in lb["items"] if x[m] is not None]
    check(f"products ranked by {m}", vals == sorted(vals, reverse=True), vals[:5])
call("GET", "/leaderboards/products?metric=nonsense", expect=400)
check("unknown metric refused", True)
top = call("GET", "/leaderboards/products?period=today&metric=revenue&limit=100")["items"]
dash_today = call("GET", "/dashboard?period=today")
check("leaderboard totals match the dashboard", abs(sum(float(x["revenue"]) for x in top) - float(dash_today["kpis"]["sales"])) < 0.01 or len(top) == 100, (len(top), dash_today["kpis"]["sales"]))
check("top product carries a medal", top and top[0]["medal"] == "Gold", top[:1])
for m in ["revenue", "transactions", "avg_sale", "orders", "customers", "new_customers", "discounts", "credit"]:
    st = call("GET", f"/leaderboards/staff?period=today&metric={m}")["items"]
    vals = [float(x[m]) for x in st if x[m] is not None]
    check(f"staff ranked by {m}", vals == sorted(vals, reverse=True), vals[:5])
call("GET", "/leaderboards/staff?period=today", token=reporter, expect=403)
check("staff board needs permission to view other employees", True)
call("GET", "/leaderboards/products?period=today&metric=profit", token=reporter, expect=403)
rep_metrics = call("GET", "/leaderboards/products?period=today", token=reporter)["metrics"]
check("profit metrics hidden without financial access", "profit" not in rep_metrics and "margin" not in rep_metrics, rep_metrics)

step("Roadmap 16: working days, trading hours, business date")
import datetime as _dt
call("POST", "/stock/receive", {"product_id": nduma, "quantity": 10, "cost_price": 250})
base_cfg = call("GET", "/settings")["settings"]
check("workspace defaults: every day, calendar day", base_cfg["workspace"]["hours"] == {"days": [True] * 7, "open": "00:00", "close": "00:00"} and base_cfg["workspace"]["outside_hours"] == "allow", base_cfg.get("workspace"))
def put_ws(hours, outside="allow", expect=200, token=None):
    c = json.loads(json.dumps(base_cfg)); c["workspace"] = {"hours": hours, "outside_hours": outside}
    return call("PUT", "/settings", c, expect=expect, token=token)
put_ws({"days": [True] * 7, "open": "25:00", "close": "02:00"}, expect=400)
put_ws({"days": [False] * 7, "open": "08:00", "close": "20:00"}, expect=400)
check("invalid hours refused", True)
put_ws({"days": [True] * 7, "open": "08:00", "close": "20:00"}, token=stocker, expect=403)
check("hours need the workspace permission", True)
cal_today = call("GET", "/dashboard?period=today")["today"]
# Day starts at 23:59 (open 24 h): right now still belongs to yesterday's business day.
put_ws({"days": [True] * 7, "open": "23:59", "close": "23:59"})
late = call("GET", "/dashboard?period=today")
yesterday = str(_dt.date.fromisoformat(cal_today) - _dt.timedelta(days=1))
check("business today follows the day start", late["today"] == yesterday, (cal_today, late["today"]))
ls = call("POST", "/sales", {"items": [{"product_id": nduma, "quantity": 1, "unit_price": 400}], "payment": {"method": "cash"}, "client_ref": str(uuid.uuid4())})
row = next(x for x in call("GET", "/sales?period=today&limit=200")["items"] if x["id"] == ls["sale"]["id"])
check("late sale stored with the business date", row["business_date"] == yesterday, row["business_date"])
check("late sale counted on its business day", any(x["id"] == ls["sale"]["id"] for x in call("GET", f"/sales?from={yesterday}&to={yesterday}&limit=200")["items"]))
mv_bd = [m for m in call("GET", f"/stock/movements?from={yesterday}&to={yesterday}&limit=200")["items"] if m["ref_id"] == ls["sale"]["id"]]
check("stock movement carries the same business date", mv_bd and all(m["business_date"] == yesterday for m in mv_bd), [m["business_date"] for m in mv_bd])
rep = call("GET", f"/reports/sales?from={yesterday}&to={yesterday}")
check("sales report includes the late sale", any(r["receipt_no"] == ls["sale"]["receipt_no"] for r in rep["rows"]))
put_ws({"days": [True] * 7, "open": "00:00", "close": "00:00"})
check("business dates already recorded never move", next(x for x in call("GET", f"/sales?from={yesterday}&to={yesterday}&limit=200")["items"] if x["id"] == ls["sale"]["id"])["business_date"] == yesterday)

# Branch override: needs the workspace permission; omitting hours keeps them.
br = next(b for b in call("GET", "/branches") if b["id"] == login["profile"]["branches"][0]["id"])
br_body = {k: br[k] for k in ("name", "code", "location", "phone", "manager_id", "is_active")}
call("PUT", f"/branches/{br['id']}", {**br_body, "hours": {"days": [True] * 6 + [False], "open": "06:00", "close": "02:00"}})
me_br = next(b for b in call("GET", "/auth/me")["branches"] if b["id"] == br["id"])
check("branch keeps its own hours", me_br["own_hours"] and me_br["hours"]["close"] == "02:00", me_br)
bm_role = call("POST", "/roles", {"name": f"Branch admin {suffix}", "permissions": ["branches.manage"]})["id"]
bm = make_user("brancher", bm_role)
call("PUT", f"/branches/{br['id']}", {**br_body, "hours": None}, token=bm, expect=403)
check("branch hours need the workspace permission", True)
call("PUT", f"/branches/{br['id']}", {**br_body, "phone": "0700000000"}, token=bm)
check("editing a branch without hours keeps them", next(b for b in call("GET", "/branches") if b["id"] == br["id"])["hours"]["close"] == "02:00")
call("PUT", f"/branches/{br['id']}", {**br_body, "hours": None})
check("branch back to the business hours", not next(b for b in call("GET", "/auth/me")["branches"] if b["id"] == br["id"])["own_hours"])

# Blocking: today is a day off; a salesperson cannot sell, a manager with the bypass can.
off = [True] * 7; off[_dt.date.fromisoformat(cal_today).weekday()] = False
put_ws({"days": off, "open": "00:00", "close": "00:00"}, outside="block")
call("POST", "/sales", {"items": [{"product_id": nduma, "quantity": 1, "unit_price": 400}], "payment": {"method": "cash"}, "client_ref": str(uuid.uuid4())}, token=seller, expect=422)
check("sale blocked outside trading hours", True)
call("POST", "/sales", {"items": [{"product_id": nduma, "quantity": 1, "unit_price": 400}], "payment": {"method": "cash"}, "client_ref": str(uuid.uuid4())})
check("bypass permission sells outside hours", True)
check("supervising template can sell after hours", "sales.outside_hours" in roles14["Manager"]["permissions"] or "sales.outside_hours" in next(r for r in call("GET", "/roles") if r["name"] == "Manager")["permissions"])
put_ws({"days": [True] * 7, "open": "00:00", "close": "00:00"}, outside="allow")
call("POST", "/sales", {"items": [{"product_id": nduma, "quantity": 1, "unit_price": 400}], "payment": {"method": "cash"}, "client_ref": str(uuid.uuid4())}, token=seller)
check("allowed again after reopening", True)

step("Roadmap 17: geofencing")
call("POST", "/stock/receive", {"product_id": nduma, "quantity": 10, "cost_price": 250})
HERE, FAR = "-1.284100,36.823300,15", "-1.264900,36.802800,15"   # at the branch / ~3 km away (Westlands)
NEAR = "-1.282300,36.823300,60"                                     # ~200 m away, ±60 m (radius 150)
def sell(token=None, location=None, expect=200):
    return call("POST", "/sales", {"items": [{"product_id": nduma, "quantity": 1, "unit_price": 400}], "payment": {"method": "cash"},
                                   "client_ref": str(uuid.uuid4())}, token=token, location=location, expect=expect)
gbr = next(b for b in call("GET", "/branches") if b["id"] == BRANCH)
g_body = {k: gbr[k] for k in ("name", "code", "location", "phone", "manager_id", "is_active")}
call("PUT", f"/branches/{BRANCH}", {**g_body, "geofence": {"latitude": None, "longitude": None, "radius_m": 150, "enabled": True}}, expect=400)
call("PUT", f"/branches/{BRANCH}", {**g_body, "geofence": {"latitude": -1.2841, "longitude": 36.8233, "radius_m": 5, "enabled": True}}, expect=400)
call("PUT", f"/branches/{BRANCH}", {**g_body, "geofence": {"latitude": 95, "longitude": 36.8233, "radius_m": 150, "enabled": False}}, expect=400)
check("invalid branch location refused", True)
call("PUT", f"/branches/{BRANCH}", {**g_body, "geofence": {"latitude": -1.2841, "longitude": 36.8233, "radius_m": 175, "enabled": True}}, token=bm, expect=403)
check("branch location needs the workspace permission", True)
call("PUT", f"/branches/{BRANCH}", {**g_body, "geofence": {"latitude": -1.2841, "longitude": 36.8233, "radius_m": 150, "enabled": True}})
check("profile shows the branch fence", next(b for b in call("GET", "/auth/me")["branches"] if b["id"] == BRANCH)["geofence"]["radius_m"] == 150)
sell(token=seller)
check("anywhere rule: fence alone restricts nothing", True)
g_cfg = call("GET", "/settings")["settings"]
def put_loc(mode, areas=None, expect=200):
    c = json.loads(json.dumps(g_cfg)); c["workspace"]["location"] = {"mode": mode, "areas": areas if areas is not None else g_cfg["workspace"]["location"]["areas"]}
    return call("PUT", "/settings", c, expect=expect)
check("default rule is anywhere for every area", g_cfg["workspace"]["location"] == {"mode": "anywhere", "areas": ["sales", "returns", "stock", "transfers", "expenses", "orders", "credit"]}, g_cfg["workspace"]["location"])
put_loc("branch", ["sales", "teleport"], expect=400)
check("unknown area refused", True)
put_loc("branch")
sell(token=seller, expect=422)
check("no location: refused at a geofenced branch", True)
sell(token=seller, location=FAR, expect=422)
check("far from the branch: refused", True)
sell(token=seller, location="-1.284100,36.823300,900", expect=422)
check("imprecise reading: refused", True)
sell(token=seller, location="here,there,1", expect=422)
check("garbled location treated as none", True)
near_sale = sell(token=seller, location=NEAR)
check("within radius (with accuracy allowance): accepted", True)
sell(token=seller, location=HERE)
spender = make_user("spender", call("POST", "/roles", {"name": f"Petty cash {suffix}", "permissions": ["expenses.create", "expenses.view"]})["id"])
call("POST", "/expenses", {"category_id": transport, "amount": 100, "description": "Bus fare"}, token=spender, location=FAR, expect=422)
call("POST", "/expenses", {"category_id": transport, "amount": 100, "description": "Bus fare"}, token=clerk, location=FAR)
check("other areas restricted too (expenses); managers may work away", True)
sell(location=FAR)
check("bypass permission works anywhere", True)
check("manager template can work away", "location.bypass" in next(r for r in call("GET", "/roles") if r["name"] == "Manager")["permissions"])
put_loc("branch", ["stock"])
sell(token=seller)
check("only chosen areas are restricted", True)
aud = call("GET", "/audit?period=today&module=sales&limit=200")["items"]
loc_entry = next((x for x in aud if x["entity_id"] == near_sale["sale"]["id"] and x["location"]), None)
check("location saved in the audit trail", loc_entry is not None and abs(loc_entry["location"]["lat"] + 1.2823) < 1e-6 and loc_entry["location"]["accuracy_m"] == 60, loc_entry and loc_entry["location"])
put_loc("anywhere")
call("PUT", f"/branches/{BRANCH}", {**g_body, "geofence": {"latitude": -1.2841, "longitude": 36.8233, "radius_m": 150, "enabled": False}})
sell(token=seller)
check("back to anywhere", True)

step("Query strings: paging & flags on every list")
for path in ["/sales?period=all&limit=5&offset=0", "/products?limit=5&offset=5&status=all", "/stock?limit=5", "/stock/movements?period=all&limit=5",
             "/stock/items?limit=5", "/stock/adjustments?period=all&limit=5", "/transfers?limit=5", "/customers?limit=5&with_credit=true",
             "/credit?status=all&limit=5", "/orders?status=all&limit=5", "/expenses?period=all&limit=5", "/approvals?status=all&mine=true&limit=5",
             "/audit?period=week&limit=5", f"/customers/{cust_id}/loyalty?limit=5", "/pos/products?all=true", "/notifications?unread=true&limit=5"]:
    try:
        call("GET", path)
        check(f"GET {path}", True)
    except AssertionError as e:
        check(f"GET {path}", False, str(e))

step("Dashboard, reports, search, notifications, audit")
d = call("GET", "/dashboard?period=today")
check("dashboard has sales", float(d["kpis"]["sales"]) > 0, d["kpis"])
cat_r = call("GET", "/reports")["reports"]
bad = []
for rep in cat_r:
    try:
        call("GET", f"/reports/{rep['key']}?period=month")
    except AssertionError as e:
        bad.append(f"{rep['key']}: {e}")
check(f"all {len(cat_r)} reports run", not bad, "; ".join(bad))
req = urllib.request.Request(f"{BASE}/api/reports/sales?period=month&format=xlsx", headers={"Authorization": "Bearer " + TOKEN})
with urllib.request.urlopen(req) as r:
    check("Excel export", r.read()[:2] == b"PK")
check("search finds product", any(x["type"] == "product" for x in call("GET", f"/search?q=Nduma {suffix}")["results"]))
check("notifications endpoint", "unread" in call("GET", "/notifications"))
check("audit trail recorded", call("GET", "/audit?period=today")["total"] > 0)
check("loyalty overview", "totals" in call("GET", "/loyalty/overview"))
check("credit aging", len(call("GET", "/credit/aging")["buckets"]) == 5)

print(f"\n{'All checks passed ✔' if not FAILURES else str(len(FAILURES)) + ' check(s) failed: ' + ', '.join(FAILURES)}")
sys.exit(1 if FAILURES else 0)
