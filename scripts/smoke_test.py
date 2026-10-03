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


def call(method, path, body=None, expect=200, token=None, branch=None):
    req = urllib.request.Request(BASE + "/api" + path.replace(" ", "%20"), method=method)
    req.add_header("Content-Type", "application/json")
    if token or TOKEN:
        req.add_header("Authorization", "Bearer " + (token or TOKEN))
    if branch or BRANCH:
        req.add_header("X-Branch-Id", branch or BRANCH)
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

step("Maker-checker: stock write-off needs approval")
roles = call("GET", "/roles")
mgr_role = next(r["id"] for r in roles if r["name"] == "Manager")
call("PUT", "/settings/workflows/stock.write_off", {"enabled": True, "approver_type": "admin"})
clerk_mail = f"clerk{suffix.lower()}@sshop.test"
call("POST", "/users", {"name": "Clerk", "email": clerk_mail, "pin": "4321", "role_id": mgr_role, "all_branches": False, "branch_ids": [BRANCH]})
clerk = call("POST", "/auth/login", {"email": clerk_mail, "pin": "4321"})["token"]
w = call("POST", "/stock/adjustments", {"product_id": nduma, "kind": "write_off", "quantity": 1, "reason": "Rotten"}, token=clerk)
check("write-off parked for approval", w["pending_approval"] is True, w)
call("POST", f"/approvals/{w['approval_id']}/approve", {"comments": "ok"})
adj = call("GET", "/stock/adjustments?period=today")["items"]
check("approved write-off applied", any(a["status"] == "applied" and a["kind"] == "write_off" for a in adj))
call("PUT", "/settings/workflows/stock.write_off", {"enabled": False, "approver_type": "admin"})

step("Ledger integrity: on_hand = Σ movements")
pos = call("GET", f"/stock/position?period=all&product_id={nduma}&branch_id={BRANCH}")["rows"][0]
lvl = call("GET", f"/stock?q=Nduma {suffix}")["items"][0]
check("position closing matches level", pos["closing"] == lvl["on_hand"], (pos["closing"], lvl["on_hand"]))

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
