#!/usr/bin/env python3
"""End-to-end smoke test for a running S'Shop server (standard library only).

Usage:
    python scripts/smoke_test.py http://localhost:8080 admin@example.com 1234

Creates test data in the target business — run it against a scratch database,
never production.
"""
import datetime
import hashlib
import hmac
import json
import os
import sys
import threading
import time
import urllib.error
import urllib.request
import uuid
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

BASE, EMAIL, PIN = (sys.argv[1:4] + [None] * 3)[:3]
if not (BASE and EMAIL and PIN):
    sys.exit(__doc__)

TOKEN = None
BRANCH = None
FAILURES = []


def call(method, path, body=None, expect=200, token=None, branch=None, location=None, repeat=False):
    req = urllib.request.Request(BASE + "/api" + path.replace(" ", "%20"), method=method)
    req.add_header("Content-Type", "application/json")
    # Each call is its own request (repeating one tests the business rules, not the duplicate guard), unless
    # `repeat` asks for an identical request — what a double click or a client retry sends.
    if not repeat:
        req.add_header("Idempotency-Key", str(uuid.uuid4()))
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


# ── Paystack stand-in (roadmap 38) ──
# When SMOKE_PAYSTACK_PORT is set, the server under test is started with PAYSTACK_BASE_URL pointing here and
# PAYSTACK_SECRET_KEY = SMOKE_PAYSTACK_SECRET, so the whole payment path runs without the real Paystack.
PS_PORT = os.environ.get("SMOKE_PAYSTACK_PORT")
PS_SECRET = os.environ.get("SMOKE_PAYSTACK_SECRET", "")
PS = {}


class _Paystack(BaseHTTPRequestHandler):
    def log_message(self, *a):
        pass

    def _send(self, code, obj):
        raw = json.dumps(obj).encode()
        self.send_response(code)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(raw)))
        self.end_headers()
        self.wfile.write(raw)

    def _authorised(self):
        if self.headers.get("Authorization") != "Bearer " + PS_SECRET:
            self._send(401, {"status": False, "message": "Invalid key"})
            return False
        return True

    def do_POST(self):
        if not self._authorised():
            return
        body = json.loads(self.rfile.read(int(self.headers.get("Content-Length", 0))) or b"{}")
        if self.path == "/transaction/initialize":
            PS[body["reference"]] = {"amount": body["amount"], "currency": body["currency"], "email": body["email"],
                                     "metadata": body.get("metadata"), "status": "ongoing"}
            return self._send(200, {"status": True, "message": "Authorization URL created", "data": {
                "authorization_url": "https://checkout.paystack.test/" + body["reference"], "access_code": "ac_" + body["reference"],
                "reference": body["reference"]}})
        self._send(404, {"status": False, "message": "Not found"})

    def do_GET(self):
        if not self._authorised():
            return
        if self.path.startswith("/transaction/verify/"):
            ref = self.path.rsplit("/", 1)[1]
            t = PS.get(ref)
            if not t:
                return self._send(400, {"status": False, "message": "Transaction reference not found"})
            paid = t["status"] == "success"
            return self._send(200, {"status": True, "message": "Verification successful", "data": {
                "id": 1, "status": t["status"], "reference": ref, "amount": t.get("paid_amount", t["amount"]), "currency": t["currency"],
                "channel": "card", "gateway_response": "Approved" if paid else "",
                "paid_at": datetime.datetime.now(datetime.timezone.utc).isoformat().replace("+00:00", "Z") if paid else None}})
        self._send(404, {"status": False, "message": "Not found"})


if PS_PORT:
    threading.Thread(target=ThreadingHTTPServer(("127.0.0.1", int(PS_PORT)), _Paystack).serve_forever, daemon=True).start()


def paystack_webhook(event, sign=True):
    raw = json.dumps(event).encode()
    req = urllib.request.Request(BASE + "/api/webhooks/paystack", data=raw, method="POST")
    req.add_header("Content-Type", "application/json")
    sig = hmac.new(PS_SECRET.encode(), raw, hashlib.sha512).hexdigest() if sign else "00" * 64
    req.add_header("x-paystack-signature", sig)
    try:
        with urllib.request.urlopen(req) as r:
            return r.status
    except urllib.error.HTTPError as e:
        return e.code


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

step("Roadmap 49: workflow changes reach pending approvals")
sales_role = next(r["id"] for r in roles if r["name"] == "Salesperson")
for who in ("ops", "fin"):
    mail = f"{who}{suffix.lower()}@sshop.test"
    call("POST", "/users", {"name": f"{who.title()} {suffix}", "email": mail, "pin": "4321", "role_id": sales_role, "all_branches": False, "branch_ids": [BRANCH]})
    users[who] = call("POST", "/auth/login", {"email": mail, "pin": "4321"})["token"]
ops, fin = users["ops"], users["fin"]
ops_id = call("GET", "/auth/me", token=ops)["user"]["id"]
fin_id = call("GET", "/auth/me", token=fin)["user"]["id"]
wf_prod = call("POST", "/products", {"name": f"Workflow {suffix}", "marked_price": 100, "cost_price": 60})["result"]["id"]
call("POST", "/stock/receive", {"product_id": wf_prod, "quantity": 10})
def write_off(reason):
    return call("POST", "/stock/adjustments", {"product_id": wf_prod, "kind": "write_off", "quantity": 1, "reason": reason}, token=clerk)["approval_id"]
def item(aid, token=None):
    # None when the request is not in that user's view (not theirs to decide, not raised by them).
    return next((a for a in call("GET", "/approvals?status=all&limit=200", token=token)["items"] if a["id"] == aid), None)
# Initiator → Manager → Finance
saved = call("PUT", "/settings/workflows/stock.write_off", {"enabled": True, "min_amount": None,
              "levels": [{"approver_type": "role", "approver_role_id": mgr_role}, {"approver_type": "user", "approver_user_id": fin_id}]})
M, F = saved["levels"]
check("workflow steps get stable ids", M.get("id") and F.get("id") and M["id"] != F["id"], saved["levels"])
w1 = write_off("Expired A")
call("POST", f"/approvals/{w1}/approve", {"comments": "manager ok"}, token=mgr2)
w2 = write_off("Expired B")
check("before the change: w1 waits for Finance", item(w1)["level"] == 2 and item(w1)["next_approvers"] == [f"Fin {suffix}"], item(w1)["next_approvers"])
same = call("PUT", "/settings/workflows/stock.write_off", {"enabled": True, "min_amount": None,
             "levels": [{"approver_type": "role", "approver_role_id": mgr_role}, {"approver_type": "user", "approver_user_id": fin_id}]})
check("saving the same workflow without step ids changes nothing", [l["id"] for l in same["levels"]] == [M["id"], F["id"]]
      and not same["affected_pending"] and item(w1)["level"] == 2, same["affected_pending"])
# Initiator → Manager → Operations → Finance
O = {"approver_type": "user", "approver_user_id": ops_id}
saved = call("PUT", "/settings/workflows/stock.write_off", {"enabled": True, "min_amount": None, "levels": [M, O, F]})
aff = {a["id"]: a for a in saved["affected_pending"]}
check("save reports the affected pending requests", w1 in aff, saved["affected_pending"])
check("w1: Manager kept, now waits for the new Operations step", aff[w1]["previous_level"] == 2 and aff[w1]["level"] == 2 and aff[w1]["levels"] == 3
      and aff[w1]["previous_next_approvers"] == [f"Fin {suffix}"] and aff[w1]["next_approvers"] == [f"Ops {suffix}"], aff[w1])
check("w2: nothing approved yet — stays at the start of the new chain", item(w2)["level"] == 1 and item(w2)["levels"] == 3, item(w2)["level"])
check("old approver no longer has it to decide (gone from their queue)", (item(w1, token=fin) or {}).get("can_decide") is not True)
check("new approver has it to decide", item(w1, token=ops)["can_decide"] is True)
check("newly responsible approver notified", any("workflow updated" in n["title"] for n in call("GET", "/notifications", token=ops)["items"]))
call("POST", f"/approvals/{w1}/approve", {"comments": "ops ok"}, token=ops)
call("POST", f"/approvals/{w1}/approve", {"comments": "finance ok"}, token=fin)
done = item(w1)
check("w1 completes once through the new chain, no approval repeated", done["status"] == "approved"
      and [d["level"] for d in done["decisions"]] == [1, 2, 3], done["decisions"])
# A new step placed before the Manager approval already given
w3 = write_off("Expired C")
call("POST", f"/approvals/{w3}/approve", {"comments": "manager ok"}, token=mgr2)
A = {"approver_type": "user", "approver_user_id": ops_id}
saved = call("PUT", "/settings/workflows/stock.write_off", {"enabled": True, "min_amount": None, "levels": [A, M, F]})
a3 = next(a for a in saved["affected_pending"] if a["id"] == w3)
check("step inserted before an approval already given: flagged, not rewritten", a3["level"] == 1 and "already given" in a3["note"], a3)
call("POST", f"/approvals/{w3}/approve", {"comments": "audit ok"}, token=ops)
check("the Manager approval already given is not asked again", item(w3)["level"] == 3 and item(w3)["next_approvers"] == [f"Fin {suffix}"], item(w3))
call("POST", f"/approvals/{w3}/approve", {"comments": "finance ok"}, token=fin)
check("w3 completes", item(w3)["status"] == "approved" and sum(1 for d in item(w3)["decisions"] if d["decision"] == "approved") == 3)
# Remaining steps removed: never approved automatically
w4 = write_off("Expired D")
first = item(w4)["steps"][0]
call("POST", f"/approvals/{w4}/approve", {"comments": "audit ok"}, token=ops)
saved = call("PUT", "/settings/workflows/stock.write_off", {"enabled": True, "min_amount": None, "levels": [first]})
a4 = next(a for a in saved["affected_pending"] if a["id"] == w4)
check("all steps already approved: waits for an administrator, nothing automatic", item(w4)["status"] == "pending" and a4["level"] == 2
      and "administrator" in a4["note"], a4)
call("POST", f"/approvals/{w4}/approve", {"comments": "admin confirms"})
check("administrator gives the final decision", item(w4)["status"] == "approved")
sync_audit = [x for x in call("GET", "/audit?period=today&module=approvals&limit=200")["items"] if x["action"] == "workflow_sync"]
check("each sync audited with previous and new next approver", any(x["before"]["next_approvers"] == [f"Fin {suffix}"] and x["after"]["next_approvers"] == [f"Ops {suffix}"] for x in sync_audit), len(sync_audit))
wf_audit = next(x for x in call("GET", "/audit?period=today&module=settings&limit=200")["items"] if x["action"] == "workflow")
check("workflow change audited with previous and new workflow and affected requests", wf_audit["before"] is not None and "affected_pending" in wf_audit["after"])
call("POST", f"/approvals/{w2}/withdraw", token=clerk)
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
check("preferences saved on the profile", me["user"]["preferences"] == {"language": "en", "font": "Poppins", "currency": "USD", "quick_sale_draggable": False}, me["user"]["preferences"])
call("PUT", "/auth/preferences", {"language": "en", "font": "Poppins", "currency": "USD", "quick_sale_draggable": True})
check("quick action preference saved (roadmap 48)", call("GET", "/auth/me")["user"]["preferences"]["quick_sale_draggable"] is True)
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

step("Record Sale: strict barcode validation")
import threading as _th
ring = call("POST", "/products", {"name": f"Ring {suffix}", "marked_price": 900, "cost_price": 500, "track_items": True})["result"]["id"]
chain = call("POST", "/products", {"name": f"Chain {suffix}", "marked_price": 700, "cost_price": 400, "track_items": True})["result"]["id"]
R = [f"RG{suffix}{i}" for i in range(5)]
call("POST", "/stock/receive", {"product_id": ring, "quantity": 4, "barcodes": R[:4]})
call("POST", "/stock/receive", {"product_id": chain, "quantity": 1, "barcodes": [f"CH{suffix}"]})
call("POST", "/stock/receive", {"product_id": ring, "quantity": 1, "barcodes": [R[4]], "branch_id": b2}, branch=b2)
def cb(code, product=ring, expect=200):
    return call("POST", "/sales/check-barcode", {"product_id": product, "barcode": code}, token=seller, expect=expect)
def title(code, product=ring):
    return cb(code, product, 422)["error"]["title"]
check("valid item accepted at scan time", cb(R[0])["ok"] is True)
check("another product's barcode: mismatch", title(f"CH{suffix}") == "Barcode mismatch")
check("unknown barcode", title(f"NOPE{suffix}") == "Unknown barcode")
check("item held at another branch", title(R[4]) == "Wrong branch")
def sell_items(codes, token=None, expect=200):
    return call("POST", "/sales", {"items": [{"product_id": ring, "quantity": 1, "unit_price": 900, "barcode": c} for c in codes],
                                   "payment": {"method": "cash"}, "client_ref": str(uuid.uuid4())}, token=token or seller, expect=expect)
check("same item twice in one sale refused", sell_items([R[1], R[1]], expect=422)["error"]["title"] == "Already in this sale")
check("checkout refuses another product's barcode", sell_items([f"CH{suffix}"], expect=422)["error"]["title"] == "Barcode mismatch")
sell_items([R[0]])
check("sold item refused at scan", title(R[0]) == "Item already sold")
check("sold item refused at checkout", sell_items([R[0]], expect=422)["error"]["title"] == "Item already sold")
tr = call("POST", "/transfers", {"to_branch_id": b2, "items": [{"product_id": ring, "quantity": 1, "barcodes": [R[3]]}], "submit": True})
call("POST", f"/transfers/{tr['id']}/dispatch")
check("item in transit refused", title(R[3]) == "Item in transit")
# Two tills sell the same unit at the same moment: exactly one succeeds.
results = []
def race():
    try:
        sell_items([R[2]]); results.append(200)
    except AssertionError as e:
        results.append(422 if "422" in str(e) else str(e))
threads = [_th.Thread(target=race) for _ in range(2)]
[t_.start() for t_ in threads]; [t_.join() for t_ in threads]
check("simultaneous sale of one unit: one wins", sorted(results) == [200, 422], results)
mv = [m for m in call("GET", f"/stock/movements?period=today&product_id={ring}&limit=50")["items"] if m["kind"] == "sale"]
check("exact units cleared with a movement each", sorted(m["barcode"] for m in mv) == sorted([R[0], R[2]]), [m["barcode"] for m in mv])
check("units cleared only by completed sales", next(x for x in call("GET", f"/stock?q=Ring {suffix}")["items"])["on_hand"] == 1)

step("Roadmap 18: transfer receipt with discrepancies")
call("POST", "/stock/receive", {"product_id": nduma, "quantity": 10, "cost_price": 250})
call("POST", "/stock/receive", {"product_id": chain, "quantity": 2, "barcodes": [f"CH{suffix}b", f"CH{suffix}c"]})
def level(pid, br):
    return call("GET", f"/stock?q=Nduma {suffix}", branch=br)["items"][0]["on_hand"]
src0, dst0 = level(nduma, BRANCH), level(nduma, b2)
dt = call("POST", "/transfers", {"to_branch_id": b2, "items": [{"product_id": nduma, "quantity": 6},
                                  {"product_id": chain, "quantity": 2, "barcodes": [f"CH{suffix}b", f"CH{suffix}c"]}], "submit": True})
call("POST", f"/transfers/{dt['id']}/dispatch")
lines = {(i["product_id"], i["barcode"]): i["id"] for i in call("GET", f"/transfers/{dt['id']}")["items"]}
nd_line, ch_damaged = lines[(nduma, None)], lines[(chain, f"CH{suffix}c")]
call("POST", f"/transfers/{dt['id']}/receive", {"lines": [{"id": nd_line, "short": 2, "damaged": 1}], "reason": ""}, branch=b2, expect=400)
check("discrepancy needs a reason", True)
call("POST", f"/transfers/{dt['id']}/receive", {"lines": [{"id": nd_line, "short": 5, "damaged": 2}], "reason": "Box crushed"}, branch=b2, expect=400)
check("cannot report more than was sent", True)
call("POST", f"/transfers/{dt['id']}/receive", {"lines": [{"id": str(uuid.uuid4()), "short": 1}], "reason": "Box crushed"}, branch=b2, expect=400)
check("lines must belong to the transfer", True)
rc = call("POST", f"/transfers/{dt['id']}/receive", {"lines": [{"id": nd_line, "short": 2, "damaged": 1}, {"id": ch_damaged, "damaged": 1}],
                                                     "reason": "2 missing from the carton, 1 crushed; chain clasp broken"}, branch=b2)
check("received with discrepancies", rc["status"] == "received" and rc["short"] == 2 and rc["damaged"] == 2, rc)
check("only good units become stock at the destination", level(nduma, b2) - dst0 == 3, (dst0, level(nduma, b2)))
check("source was reduced by everything sent", src0 - level(nduma, BRANCH) == 6, (src0, level(nduma, BRANCH)))
dd = call("GET", f"/transfers/{dt['id']}")
nl = next(i for i in dd["items"] if i["id"] == nd_line)
check("line shows received / short / damaged", (nl["received_qty"], nl["short_qty"], nl["damaged_qty"]) == (3, 2, 1), nl)
check("transfer keeps the reason", dd["transfer"]["short_units"] == 2 and dd["transfer"]["damaged_units"] == 2 and "carton" in dd["transfer"]["discrepancy_reason"])
mv_t = [m for m in call("GET", f"/stock/movements?period=today&limit=200&branch_id={b2}", branch=b2)["items"] if m["ref_id"] == dt["id"]]
kinds = sorted((m["kind"], m["quantity"]) for m in mv_t if m["product_id"] == nduma)
check("ledger: in 6, loss 2, damage 1", kinds == [("damage", -1), ("loss", -2), ("transfer_in", 6)], kinds)
check("damaged tracked unit written off, good one in stock",
      title(f"CH{suffix}b", chain) == "Wrong branch" and title(f"CH{suffix}c", chain) == "Item written off")
aud_t = [x for x in call("GET", f"/audit?period=today&entity_id={dt['id']}&limit=20")["items"] if x["action"] == "receive"]
check("receipt audited with the discrepancy", aud_t and "carton" in aud_t[0]["comments"], aud_t[:1])
ok_t = call("POST", "/transfers", {"to_branch_id": b2, "items": [{"product_id": nduma, "quantity": 1}], "submit": True})
call("POST", f"/transfers/{ok_t['id']}/dispatch")
check("receipt with no body still receives everything", call("POST", f"/transfers/{ok_t['id']}/receive", branch=b2)["status"] == "received")

step("Product photos: validation, limit, retry-safe uploads")
def upload(pid, data, ref=None, expect=200):
    boundary = "sshop" + uuid.uuid4().hex
    parts = []
    if ref:
        parts.append(f'--{boundary}\r\nContent-Disposition: form-data; name="upload_ref"\r\n\r\n{ref}\r\n'.encode())
    parts.append(f'--{boundary}\r\nContent-Disposition: form-data; name="file"; filename="p.png"\r\nContent-Type: image/png\r\n\r\n'.encode() + data + b"\r\n")
    parts.append(f"--{boundary}--\r\n".encode())
    req = urllib.request.Request(BASE + f"/api/products/{pid}/photos", data=b"".join(parts), method="POST")
    req.add_header("Authorization", "Bearer " + TOKEN)
    req.add_header("Content-Type", f"multipart/form-data; boundary={boundary}")
    try:
        with urllib.request.urlopen(req) as r:
            status, raw = r.status, r.read()
    except urllib.error.HTTPError as e:
        status, raw = e.code, e.read()
    payload = json.loads(raw) if raw[:1] == b"{" else raw
    if status != expect:
        raise AssertionError(f"photo upload → {status} (expected {expect}): {payload}")
    return payload
PNG = b"\x89PNG\r\n\x1a\n" + b"\x00" * 2000
ph_prod = call("POST", "/products", {"name": f"Photo Test {suffix}", "marked_price": 100})["result"]["id"]
ref = str(uuid.uuid4())
first = upload(ph_prod, PNG, ref)
again = upload(ph_prod, PNG, ref)
check("same pending photo sent twice is stored once", again["id"] == first["id"] and again.get("duplicate") is True)
check("not-an-image refused", upload(ph_prod, b"%PDF-1.4 hello", str(uuid.uuid4()), expect=422)["error"]["title"] == "Not a photo")
check("2.5 MB photo accepted (body limit fits the 3 MB rule)", "id" in upload(ph_prod, PNG + b"\x00" * (2_500_000), str(uuid.uuid4())))
check("over 3 MB refused", upload(ph_prod, PNG + b"\x00" * (3_200_000), str(uuid.uuid4()), expect=422)["error"]["title"] == "Photo too large")
limit = call("GET", "/settings")["settings"]["product"]["max_photos"]
for _ in range(limit - 2):
    upload(ph_prod, PNG, str(uuid.uuid4()))
check("limit enforced", upload(ph_prod, PNG, str(uuid.uuid4()), expect=422)["error"]["title"] == "Photo limit reached")
check("photo count matches what was stored", len(call("GET", f"/products/{ph_prod}")["photos"]) == limit)

step("Credit Sales: recall to stock")
call("POST", "/stock/receive", {"product_id": nduma, "quantity": 5, "cost_price": 250})
RC = [f"RC{suffix}a", f"RC{suffix}b"]
call("POST", "/stock/receive", {"product_id": ring, "quantity": 2, "barcodes": RC})
rcs = call("POST", "/sales", {"customer_id": cust_id, "items": [{"product_id": nduma, "quantity": 3, "unit_price": 400},
        {"product_id": ring, "quantity": 1, "unit_price": 900, "barcode": RC[0]}], "payment": {"method": "credit"}, "client_ref": str(uuid.uuid4())})
rc_id = rcs["credit"]["id"]
rc = call("GET", f"/credit/{rc_id}")
nd_item = next(i for i in rc["items"] if not i["tracked"])
rg_item = next(i for i in rc["items"] if i["tracked"])
check("credit detail lists what was sold", rc["can_recall"] and rg_item["barcode"] == RC[0] and nd_item["quantity"] == 3)
def recall(items, reason="Customer could not pay", settle="", expect=200, token=None, **kw):
    return call("POST", f"/credit/{rc_id}/recall", {"items": items, "reason": reason, "settle": settle, **kw}, token=token, expect=expect)
call("POST", f"/credit/{rc_id}/recall", {"items": [{"sale_item_id": nd_item["id"], "quantity": 1}], "reason": "x"}, token=reporter, expect=403)
check("recall needs permission", True)
check("reason required", recall([{"sale_item_id": nd_item["id"], "quantity": 1}], reason="", expect=400) is not None)
check("tracked unit must be scanned", recall([{"sale_item_id": rg_item["id"], "quantity": 1}], expect=422)["error"]["title"] == "Scan required")
check("wrong barcode refused", recall([{"sale_item_id": rg_item["id"], "quantity": 1, "barcodes": [RC[1]]}], expect=422)["error"]["title"] == "Barcode mismatch")
check("cannot recall more than sold", recall([{"sale_item_id": nd_item["id"], "quantity": 4}], expect=422)["error"]["title"] == "Too many")
nd_before = call("GET", f"/stock?q=Nduma {suffix}")["items"][0]["on_hand"]
r1 = recall([{"sale_item_id": nd_item["id"], "quantity": 1}], reason="One bag returned unopened")
rc = call("GET", f"/credit/{rc_id}")
check("partial recall reduces the balance", float(rc["credit"]["balance"]) == 1200 + 900 - 400 - 0 and rc["credit"]["recall_state"] == "partially_recalled", rc["credit"])
check("collection continues on what is still owed", rc["credit"]["status"] in ("outstanding", "overdue"))
check("stock back at the original branch", call("GET", f"/stock?q=Nduma {suffix}")["items"][0]["on_hand"] == nd_before + 1)
mvr = [m for m in call("GET", "/stock/movements?period=today&limit=100")["items"] if "Credit sale recall" in (m["notes"] or "")]
check("ledger shows the recall", any(m["kind"] == "customer_return" and m["quantity"] == 1 for m in mvr))
call("POST", f"/credit/{rc_id}/payments", {"amount": 1500, "method": "cash"})
over = recall([{"sale_item_id": nd_item["id"], "quantity": 2}, {"sale_item_id": rg_item["id"], "quantity": 1, "barcodes": [RC[0]]}], expect=422)
check("overpayment must be settled explicitly", over["error"]["title"] == "Customer has overpaid", over)
r2 = recall([{"sale_item_id": nd_item["id"], "quantity": 2}, {"sale_item_id": rg_item["id"], "quantity": 1, "barcodes": [RC[0]]}],
            reason="Goods collected back", settle="credit")
rc = call("GET", f"/credit/{rc_id}")
last = rc["recalls"][-1]
check("full recall closes the credit as Recalled", rc["credit"]["status"] == "recalled" and rc["credit"]["recall_state"] == "recalled", rc["credit"])
check("overpayment kept as customer credit (no payout)", float(last["customer_credit"]) == 1500 and float(last["balance_after"]) == 0, last)
check("payment history untouched", [float(x["amount"]) for x in rc["payments"]] == [1500.0], rc["payments"])
check("exact unit back in stock", call("POST", "/sales/check-barcode", {"product_id": ring, "barcode": RC[0]})["ok"] is True)
check("nothing left to recall", recall([{"sale_item_id": nd_item["id"], "quantity": 1}], expect=422)["error"]["title"] == "Cannot recall")
aud_r = call("GET", f"/audit?period=today&entity_id={rc_id}&limit=20")["items"]
check("recall audited with balances", any(x["action"] == "recall" and x["after"]["balance_before"] is not None for x in aud_r))

step("Hardening: another business cannot reach this one")
# The administrator of a second business (all permissions there) attacks this business's records by id.
_r = urllib.request.Request(BASE + "/api/auth/me"); _r.add_header("Authorization", "Bearer " + t2["token"])
x_branch = json.load(urllib.request.urlopen(_r))["branches"][0]["id"]
x_appr = call("GET", "/approvals?status=all&limit=1")["items"][0]["id"]
x_exp = call("POST", "/expenses", {"category_id": rent, "amount": 50, "description": "Water"})["result"]["id"]
def attack(method, path, body=None):
    req = urllib.request.Request(BASE + "/api" + path, method=method, data=json.dumps(body).encode() if body is not None else None)
    req.add_header("Content-Type", "application/json")
    req.add_header("Authorization", "Bearer " + t2["token"])
    req.add_header("X-Branch-Id", x_branch)
    try:
        with urllib.request.urlopen(req) as r:
            raw = r.read()
            body_ = json.loads(raw) if raw[:1] in (b"{", b"[") else raw
            # A list scoped to the caller's own business may answer 200 — but it must be empty.
            empty = body_ == [] or (isinstance(body_, dict) and body_.get("items") == [])
            return 404 if empty else r.status
    except urllib.error.HTTPError as e:
        return e.code
targets = [
    ("GET", f"/products/{nduma}"), ("PUT", f"/products/{nduma}", {"name": "HACKED", "marked_price": 1}),
    ("POST", f"/products/{nduma}/status", {"is_active": False}), ("DELETE", f"/products/{ph_prod}/photos/{first['id']}"),
    ("GET", f"/sales/{mine_sale['sale']['id']}"), ("POST", f"/sales/{mine_sale['sale']['id']}/cancel", {"reason": "x"}),
    ("POST", f"/sales/{mine_sale['sale']['id']}/return", {"items": [], "reason": "x"}),
    ("GET", f"/customers/{cust_id}"), ("PUT", f"/customers/{cust_id}", {"first_name": "HACKED"}),
    ("GET", f"/credit/{credit_id}"), ("POST", f"/credit/{credit_id}/payments", {"amount": 1, "method": "cash"}),
    ("POST", f"/credit/{credit_id}/write-off", {"reason": "x"}), ("POST", f"/credit/{rc_id}/recall", {"items": [], "reason": "xxxx"}),
    ("GET", f"/orders/{o2['id']}"), ("POST", f"/orders/{o2['id']}/status", {"status": "cancelled"}),
    ("GET", f"/transfers/{dt['id']}"), ("POST", f"/transfers/{ok_t['id']}/receive"), ("POST", f"/transfers/{dt['id']}/cancel", {"reason": "x"}),
    ("POST", f"/expenses/{x_exp}/void"),
    ("POST", f"/approvals/{x_appr}/approve", {"comments": "x"}), ("POST", f"/approvals/{x_appr}/reject", {"comments": "x"}),
    ("PUT", f"/branches/{BRANCH}", {"name": "HACKED", "code": "HX"}), ("PUT", f"/roles/{repo_role}", {"name": "HACKED", "permissions": ["*"]}),
    ("PUT", f"/users/{admin_id}", {"name": "HACKED", "email": "x@x.test", "phone": "", "role_id": repo_role, "all_branches": True, "branch_ids": []}),
    ("POST", f"/users/{admin_id}/reset-pin", {"pin": "9999"}),
    ("POST", "/sales/check-barcode", {"product_id": ring, "barcode": RC[1]}),
    ("POST", "/stock/receive", {"product_id": nduma, "quantity": 5, "branch_id": BRANCH}),
    ("GET", f"/stock/availability/{nduma}"), ("GET", f"/audit?entity_id={nduma}"),
    ("GET", f"/dashboard?branch_id={BRANCH}"), ("GET", f"/reports/sales?branch_id={BRANCH}"),
    ("GET", f"/leaderboards/products?branch_id={BRANCH}"),
]
leaks = []
for t_ in targets:
    st = attack(*t_)
    if st < 400 or st >= 500:
        leaks.append(f"{t_[0]} {t_[1]} → {st}")
check(f"{len(targets)} cross-business attempts all refused", not leaks, leaks)
check("nothing changed by the attempts", call("GET", f"/products/{nduma}")["product"]["name"] == f"Nduma {suffix}"
      and next(b_ for b_ in call("GET", "/branches") if b_["id"] == BRANCH)["name"] != "HACKED")
check("lists never show another business's records",
      all(x["id"] != nduma for x in call("GET", "/products?status=all&limit=200", token=t2["token"], branch=x_branch)["items"]))

step("Hardening: rate limits and health")
codes = []
for i in range(32):
    req = urllib.request.Request(BASE + "/api/auth/login", method="POST", data=json.dumps({"email": f"nobody{i}@sshop.test", "pin": "0000"}).encode())
    req.add_header("Content-Type", "application/json")
    req.add_header("X-Forwarded-For", f"203.0.113.{int(suffix, 36) % 250 if suffix.isalnum() else 7}")
    try:
        urllib.request.urlopen(req); codes.append(200)
    except urllib.error.HTTPError as e:
        codes.append(e.code)
check("sign-in rate limited per client", codes[-1] == 429 and codes[0] == 400, codes[-3:])
check("health check pings the database", urllib.request.urlopen(BASE + "/healthz").read() == b"ok")

step("Offline POS: queued sales sync with their real time")
import datetime as _dt2
call("POST", "/stock/receive", {"product_id": nduma, "quantity": 5, "cost_price": 250})
def iso(dt): return dt.astimezone(_dt2.timezone.utc).isoformat().replace("+00:00", "Z")
now_ = _dt2.datetime.now(_dt2.timezone.utc)
off_ref = str(uuid.uuid4())
off_body = {"items": [{"product_id": nduma, "quantity": 1, "unit_price": 400}], "payment": {"method": "cash"},
            "client_ref": off_ref, "offline_at": iso(now_ - _dt2.timedelta(hours=2))}
o1 = call("POST", "/sales", off_body, token=seller)
o2_ = call("POST", "/sales", off_body, token=seller)
check("synced twice, recorded once", o1["sale"]["id"] == o2_["sale"]["id"])
row = next(x for x in call("GET", "/sales?period=all&limit=200")["items"] if x["id"] == o1["sale"]["id"])
sold = _dt2.datetime.fromisoformat(row["created_at"].replace("Z", "+00:00"))
check("sale keeps the moment it was made", abs((sold - (now_ - _dt2.timedelta(hours=2))).total_seconds()) < 5 and row["synced_at"], row)
mv_o = [m for m in call("GET", "/stock/movements?period=all&limit=200")["items"] if m["ref_id"] == o1["sale"]["id"]]
check("stock movement carries the same time", mv_o and abs((_dt2.datetime.fromisoformat(mv_o[0]["created_at"].replace("Z", "+00:00")) - sold).total_seconds()) < 5)
old = dict(off_body, client_ref=str(uuid.uuid4()), offline_at=iso(now_ - _dt2.timedelta(hours=80)))
check("too old to sync is refused", call("POST", "/sales", old, token=seller, expect=422)["error"]["title"] == "Offline sale too old")
fut = dict(off_body, client_ref=str(uuid.uuid4()), offline_at=iso(now_ + _dt2.timedelta(hours=2)))
check("future time refused", call("POST", "/sales", fut, token=seller, expect=422)["error"]["title"] == "Offline sale too old")
live = dict(off_body, client_ref=str(uuid.uuid4()), customer_id=cust_id, payment={"method": "credit"})
check("credit cannot be recorded offline", call("POST", "/sales", live, token=seller, expect=422)["error"]["title"] == "Needs a connection")
call("POST", "/sales", {k: v for k, v in off_body.items() if k != "client_ref"}, token=seller, expect=400)
check("offline sales need a client reference", True)
check("sync is audited", any(x["action"] == "offline_sync" for x in call("GET", f"/audit?period=all&entity_id={o1['sale']['id']}&limit=20")["items"]))

step("Record Sale: manual M-Pesa (code optional) and STK separation")
call("POST", "/stock/receive", {"product_id": nduma, "quantity": 6, "cost_price": 250})
def mp(ref="", expect=200):
    return call("POST", "/sales", {"items": [{"product_id": nduma, "quantity": 1, "unit_price": 400}],
                                   "payment": {"method": "mpesa", "reference": ref, "phone": ""}, "client_ref": str(uuid.uuid4())}, expect=expect)
check("M-Pesa sale without number or code", mp()["sale"]["payment_method"] == "mpesa")
check("malformed code refused", mp("BAD", 422)["error"]["title"] == "Check the M-Pesa code")
code = ("QF" + uuid.uuid4().hex[:8]).upper()
mp(code)
check("valid code accepted once", True)
check("same code cannot be used twice", mp(code.lower(), 422)["error"]["title"] == "M-Pesa code already used")
off_mp = {"items": [{"product_id": nduma, "quantity": 1, "unit_price": 400}], "payment": {"method": "mpesa", "reference": ""},
          "client_ref": str(uuid.uuid4()), "offline_at": iso(_dt2.datetime.now(_dt2.timezone.utc) - _dt2.timedelta(minutes=10))}
check("manual M-Pesa can be recorded offline", call("POST", "/sales", off_mp)["sale"]["payment_method"] == "mpesa")

step("Ordering link: show product prices")
pcfg = call("GET", "/settings")["settings"]
check("prices shown by default", pcfg["orders"]["show_prices"] is True and call("GET", f"/portal/{slug}", token="none")["show_prices"] is True)
def has_price(v):
    if isinstance(v, dict):
        return any(k in ("price", "unit_price", "line_total", "total", "marked_price") for k in v) or any(has_price(x) for x in v.values())
    if isinstance(v, list):
        return any(has_price(x) for x in v)
    return False
pc = json.loads(json.dumps(pcfg)); pc["orders"]["show_prices"] = False
call("PUT", "/settings", pc)
cat_np = call("GET", f"/portal/{slug}/catalogue", token=ptoken)
check("catalogue sends no prices", not has_price(cat_np) and len(cat_np["products"]) > 0)
check("product detail sends no prices", not has_price(call("GET", f"/portal/{slug}/products/{nduma}", token=ptoken)))
tr_np = call("GET", f"/portal/track/{o2['track_token']}", token="none")
check("tracking sends no prices", not has_price(tr_np) and tr_np["business"]["show_prices"] is False)
check("staff screens keep prices", call("GET", f"/products/{nduma}")["product"]["marked_price"] is not None)
pc["orders"]["show_prices"] = True
call("PUT", "/settings", pc)
check("prices back when switched on", has_price(call("GET", f"/portal/{slug}/catalogue", token=ptoken)))

step("Exchange: return and sell in one step, difference only")
mug = call("POST", "/products", {"name": f"Mug {suffix}", "marked_price": 600, "cost_price": 300})["result"]["id"]
pen = call("POST", "/products", {"name": f"Pen {suffix}", "marked_price": 100, "cost_price": 40})["result"]["id"]
call("POST", "/stock/receive", {"product_id": mug, "quantity": 5, "cost_price": 300})
call("POST", "/stock/receive", {"product_id": pen, "quantity": 5, "cost_price": 40})
call("POST", "/stock/receive", {"product_id": nduma, "quantity": 4, "cost_price": 250})
def on_hand(name):
    return call("GET", f"/stock?q={name} {suffix}")["items"][0]["on_hand"]
xs = call("POST", "/sales", {"items": [{"product_id": nduma, "quantity": 2, "unit_price": 400}], "payment": {"method": "cash"}, "client_ref": str(uuid.uuid4())})
xs_id = xs["sale"]["id"]
nd_line_x = xs["items"][0]["id"]
nd0, mug0, pen0 = on_hand("Nduma"), on_hand("Mug"), on_hand("Pen")
def ex(body, expect=200):
    base_ = {"return_items": [{"sale_item_id": nd_line_x, "quantity": 1}], "payment": {"method": "cash"}, "reason": "Wrong item", "client_ref": str(uuid.uuid4())}
    return call("POST", f"/sales/{xs_id}/exchange", {**base_, **body}, expect=expect)
call("POST", f"/sales/{xs_id}/exchange", {"return_items": [{"sale_item_id": nd_line_x, "quantity": 1}], "items": [{"product_id": mug, "quantity": 1, "unit_price": 600}],
     "payment": {"method": "cash"}, "reason": ""}, expect=400)
check("exchange needs a reason", True)
ref_a = str(uuid.uuid4())
xa = ex({"items": [{"product_id": mug, "quantity": 1, "unit_price": 600}], "client_ref": ref_a})
pays_a = sorted((p_["method"], float(p_["amount"])) for p_ in xa["payments"])
check("customer pays only the difference", float(xa["sale"]["total"]) == 600 and pays_a == [("cash", 200.0), ("exchange", 400.0)], pays_a)
check("retry returns the same exchange", ex({"items": [{"product_id": mug, "quantity": 1, "unit_price": 600}], "client_ref": ref_a})["sale"]["id"] == xa["sale"]["id"])
check("stock moved both ways", on_hand("Nduma") == nd0 + 1 and on_hand("Mug") == mug0 - 1, (on_hand("Nduma"), on_hand("Mug")))
check("surplus needs a refund method", ex({"items": [{"product_id": pen, "quantity": 1, "unit_price": 100}], "refund_method": ""}, 422)["error"]["title"] == "Choose the refund method")
xb = ex({"items": [{"product_id": pen, "quantity": 1, "unit_price": 100}], "refund_method": "cash"})
pays_b = sorted((p_["method"], float(p_["amount"])) for p_ in xb["payments"])
check("surplus refunded; payments equal the new total", pays_b == [("cash", -300.0), ("exchange", 400.0)] and sum(a_ for _, a_ in pays_b) == 100.0, pays_b)
orig = call("GET", f"/sales/{xs_id}")
exch_total = sum(float(p_["amount"]) for d in (orig, xa, xb) for p_ in d["payments"] if p_["method"] == "exchange")
check("exchange payments net to zero", abs(exch_total) < 0.01, exch_total)
check("original sale fully returned", orig["sale"]["status"] == "returned" and on_hand("Pen") == pen0 - 1)
cr_sale = call("POST", "/sales", {"customer_id": cust_id, "items": [{"product_id": nduma, "quantity": 1, "unit_price": 400}], "payment": {"method": "credit"}, "client_ref": str(uuid.uuid4())})
check("credit sales use recall instead", call("POST", f"/sales/{cr_sale['sale']['id']}/exchange", {"return_items": [{"sale_item_id": cr_sale["items"][0]["id"], "quantity": 1}],
      "items": [{"product_id": pen, "quantity": 1, "unit_price": 100}], "payment": {"method": "cash"}, "reason": "swap"}, expect=422)["error"]["title"] == "Credit sale")
check("exchange audited", any(x["action"] == "exchange" for x in call("GET", f"/audit?period=today&entity_id={xa['sale']['id']}&limit=10")["items"]))

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

step("Roadmap 34–36: platform owner — directory, activity, PIN reset, activation")
dk = me2["tenant"]["id"]
dk_admin = me2["user"]["id"]
dk_slug = me2["tenant"]["slug"]
dk_email = me2["user"]["email"]
dk_name = me2["tenant"]["name"]
home_id = login["profile"]["tenant"]["id"]
dk_row = next(x for x in call("GET", "/platform/tenants")["items"] if x["id"] == dk)
check("directory: admin contact, status, activation, billing", dk_row["admin_email"] == dk_email and dk_row["status"] == "active"
      and dk_row["activated_at"] and dk_row["billing"]["status"] == "not_set", dk_row)
dd = call("GET", f"/platform/tenants/{dk}")
check("detail: onboarding details from the access request", dd["onboarding"]["business_type"] == "Retail shop" and dd["onboarding"]["branches"] == 2, dd["onboarding"])
check("detail: users, admins and branches", any(u["id"] == dk_admin and u["is_admin"] for u in dd["users"]) and len(dd["branches"]) >= 1)
denied = []
for m_, p_, b_ in [("GET", "/platform/tenants", None), ("GET", f"/platform/tenants/{home_id}", None), ("GET", "/platform/activity", None),
                   ("GET", "/platform/billing", None), ("GET", "/platform/billing/vendor", None),
                   ("POST", f"/platform/tenants/{home_id}/status", {"status": "deactivated", "reason": "takeover"}),
                   ("POST", f"/platform/tenants/{home_id}/users/{dk_admin}/reset-pin", None),
                   ("PUT", f"/platform/tenants/{dk}/billing-plan", {"model": "subscription", "amount": 1, "start_date": "2026-01-01"}),
                   ("POST", f"/platform/tenants/{dk}/billing-documents", {"kind": "invoice", "category": "other", "amount": 1, "description": "self"})]:
    try:
        call(m_, p_, b_, token=t2["token"], branch="none", expect=403)
    except AssertionError as e:
        denied.append(str(e))
check("a business administrator never reaches platform-owner endpoints", not denied, denied)
call("POST", "/auth/login", {"email": dk_email, "pin": "wrong-pin"}, expect=400)
fails = call("GET", f"/platform/activity?activity=login_failed&tenant_id={dk}&period=today")
check("failed sign-in recorded and visible to the platform owner", fails["total"] >= 1 and fails["items"][0]["business"] == dk_name, fails)
logins = call("GET", f"/platform/activity?activity=login&tenant_id={dk}&user_id={dk_admin}&period=today")
check("sign-ins filtered by business and user", logins["total"] >= 1 and all(i["user_id"] == dk_admin for i in logins["items"]), logins["total"])
sales_act = call("GET", f"/platform/activity?activity=sale&tenant_id={home_id}&period=today")
check("sales activity across businesses", sales_act["totals"]["sale"] >= 1 and sales_act["totals"]["login"] == 0, sales_act["totals"])
call("GET", "/platform/activity?activity=nonsense", expect=400)
rp = call("POST", f"/platform/tenants/{dk}/users/{dk_admin}/reset-pin")
call("POST", "/auth/login", {"email": dk_email, "pin": ap["temporary_pin"]}, expect=400)
t2 = call("POST", "/auth/login", {"email": dk_email, "pin": rp["temporary_pin"]})
check("platform PIN reset: old PIN refused, one-time PIN works", t2["profile"]["tenant"]["id"] == dk)
check("PIN reset in the business's own audit trail", any(a["action"] == "reset_pin" and a["module"] == "platform"
      for a in call("GET", "/audit?period=today&module=platform", token=t2["token"], branch="none")["items"]))

step("Roadmap 37–39: billing plans, invoices, Paystack, receipts")
today_ = datetime.date.today().isoformat()
call("PUT", f"/platform/tenants/{dk}/billing-plan", {"model": "subscription", "amount": 0, "start_date": today_}, expect=400)
check("subscription without an amount refused", True)
call("PUT", f"/platform/tenants/{dk}/billing-plan", {"model": "subscription", "amount": 2500, "frequency": "weekly", "start_date": today_}, expect=400)
check("unknown frequency refused", True)
call("PUT", f"/platform/tenants/{dk}/billing-plan", {"model": "subscription", "amount": 2500, "frequency": "monthly", "start_date": today_,
                                                     "next_due_date": today_, "grace_days": 5, "auto_renew": True, "notes": "internal note"})
T2 = {"token": t2["token"], "branch": "none"}
mb = call("GET", "/billing", **T2)
check("business sees its plan and status", mb["plan"]["model"] == "subscription" and float(mb["plan"]["amount"]) == 2500 and mb["summary"]["status"] == "payment_due", mb["summary"])
check("internal plan notes are not shown to the business", "notes" not in mb["plan"])
check("vendor bank details masked", mb["vendor"]["bank_name"] == "I&M Bank" and mb["vendor"]["account_masked"] == "•••450" and "account_number" not in mb["vendor"], mb["vendor"])
inv = call("POST", f"/platform/tenants/{dk}/billing-documents", {"kind": "invoice", "category": "next_period"})
check("invoice for the next period", inv["number"].startswith("INV-"), inv)
call("POST", f"/platform/tenants/{dk}/billing-documents", {"kind": "invoice", "category": "next_period"}, expect=422)
check("the same period cannot be invoiced twice", True)
mb = call("GET", "/billing", **T2)
inv_doc = next(d for d in mb["documents"] if d["id"] == inv["id"])
check("invoice: number, period, due date, status", inv_doc["status"] == "open" and inv_doc["period_start"] == today_ and inv_doc["due_date"] == today_
      and float(mb["summary"]["outstanding"]) == 2500, inv_doc)
call("GET", f"/billing/documents/{inv['id']}", expect=404)
check("another business cannot open this invoice", True)
call("POST", f"/billing/invoices/{inv['id']}/pay", expect=404)
check("another business cannot pay this invoice", True)
clerk_bill = call("GET", "/billing", token=clerk, expect=403)
check("billing needs the settings.billing permission", True)
next_due_before = mb["plan"]["next_due_date"]
if PS_PORT:
    st_ = call("POST", f"/billing/invoices/{inv['id']}/pay", **T2)
    ref = st_["reference"]
    check("Pay now opens Paystack for this invoice only", st_["authorization_url"].endswith(ref) and PS[ref]["amount"] == 250000
          and PS[ref]["currency"] == "KES" and PS[ref]["metadata"]["invoice_id"] == inv["id"], PS.get(ref))
    v = call("POST", "/billing/paystack/verify", {"reference": ref}, **T2)
    check("not paid yet → still pending (the return page is not proof)", v["status"] == "pending", v)
    PS[ref]["status"] = "success"
    v = call("POST", "/billing/paystack/verify", {"reference": ref}, **T2)
    check("verified with Paystack → success with a receipt", v["status"] == "success" and v["receipt_no"].startswith("RCT-"), v)
    v2 = call("POST", "/billing/paystack/verify", {"reference": ref}, **T2)
    check("verifying again changes nothing", v2["receipt_no"] == v["receipt_no"], v2)
    mb = call("GET", "/billing", **T2)
    check("invoice paid → payment → period → next due", next(d for d in mb["documents"] if d["id"] == inv["id"])["status"] == "paid"
          and mb["plan"]["next_due_date"] > next_due_before and mb["summary"]["period_start"] == today_ and float(mb["summary"]["outstanding"]) == 0,
          (mb["plan"], mb["summary"]))
    call("POST", f"/billing/invoices/{inv['id']}/pay", **T2, expect=422)
    check("a paid invoice cannot be paid again", True)
    inv2 = call("POST", f"/platform/tenants/{dk}/billing-documents", {"kind": "invoice", "category": "next_period"})
    ref2 = call("POST", f"/billing/invoices/{inv2['id']}/pay", **T2)["reference"]
    PS[ref2]["status"] = "success"
    check("webhook with a bad signature refused", paystack_webhook({"event": "charge.success", "data": {"reference": ref2}}, sign=False) == 401)
    check("signed webhook accepted", paystack_webhook({"event": "charge.success", "data": {"reference": ref2, "amount": 1}}) == 200)
    mb = call("GET", "/billing", **T2)
    check("webhook settles after re-verifying with Paystack", next(d for d in mb["documents"] if d["id"] == inv2["id"])["status"] == "paid")
    inv3 = call("POST", f"/platform/tenants/{dk}/billing-documents", {"kind": "invoice", "category": "other", "amount": 100, "description": "Extra training"})
    ref3 = call("POST", f"/billing/invoices/{inv3['id']}/pay", **T2)["reference"]
    PS[ref3].update(status="success", paid_amount=1000)
    v = call("POST", "/billing/paystack/verify", {"reference": ref3}, **T2)
    mb = call("GET", "/billing", **T2)
    check("an amount that does not match the invoice is not accepted", v["status"] == "failed"
          and next(d for d in mb["documents"] if d["id"] == inv3["id"])["status"] == "open", v)
    call("POST", f"/platform/billing/documents/{inv3['id']}/void", {"reason": "Raised in error"})
else:
    call("POST", f"/billing/invoices/{inv['id']}/pay", **T2, expect=422)
    check("Pay now refused clearly when Paystack is not configured", True)
    call("POST", f"/platform/billing/documents/{inv['id']}/payments", {"method": "bank", "reference": f"BNK0{suffix}"})
manual = call("POST", f"/platform/tenants/{dk}/billing-documents", {"kind": "invoice", "category": "other", "amount": 300, "description": "Data migration"})
call("POST", f"/platform/billing/documents/{manual['id']}/payments", {"method": "bank", "reference": ""}, expect=400)
check("manual payment needs a reference", True)
mp_ = call("POST", f"/platform/billing/documents/{manual['id']}/payments", {"method": "bank", "reference": f"BNK{suffix}", "note": "I&M transfer"})
check("bank payment recorded by the platform owner → receipt", mp_["receipt_no"].startswith("RCT-"), mp_)
call("POST", f"/platform/billing/documents/{manual['id']}/payments", {"method": "bank", "reference": f"BNK{suffix}X"}, expect=422)
check("a paid invoice cannot take another payment", True)
doc = call("GET", f"/billing/documents/{manual['id']}", **T2)
check("receipt details for download", doc["document"]["status"] == "paid" and doc["payments"][0]["receipt_no"] == mp_["receipt_no"]
      and doc["business"]["name"] == dk_name and doc["vendor"]["account_masked"] == "•••450", doc["payments"])
quo = call("POST", f"/platform/tenants/{dk}/billing-documents", {"kind": "quotation", "category": "other", "amount": 1000, "description": "Extra branch setup"})
check("quotation numbered separately", quo["number"].startswith("QUO-"), quo)
acc = call("POST", f"/billing/quotations/{quo['id']}/accept", **T2)
call("POST", f"/billing/quotations/{quo['id']}/accept", **T2, expect=422)
check("quotation → invoice once", acc["number"].startswith("INV-"), acc)
call("POST", f"/platform/billing/documents/{acc['invoice_id']}/void", {"reason": ""}, expect=422)
call("POST", f"/platform/billing/documents/{acc['invoice_id']}/void", {"reason": "Customer changed plan"})
mb = call("GET", "/billing", **T2)
check("voided invoice kept with its reason", next(d for d in mb["documents"] if d["id"] == acc["invoice_id"])["status"] == "void")
call("PUT", f"/platform/tenants/{dk}/billing-plan", {"model": "one_off", "one_off_amount": 0}, expect=400)
call("PUT", f"/platform/tenants/{dk}/billing-plan", {"model": "one_off", "one_off_amount": 50000, "maintenance": True, "amount": 5000,
                                                     "frequency": "annual", "start_date": today_})
mb = call("GET", "/billing", **T2)
check("one-off model: one-off pending, maintenance fee and next due", mb["summary"]["one_off_status"] == "pending" and mb["summary"]["maintenance"]
      and float(mb["summary"]["amount"]) == 5000 and mb["summary"]["next_due"] == today_, mb["summary"])
oo = call("POST", f"/platform/tenants/{dk}/billing-documents", {"kind": "invoice", "category": "one_off"})
call("POST", f"/platform/billing/documents/{oo['id']}/payments", {"method": "mpesa", "reference": f"MP{suffix}OO"})
mb = call("GET", "/billing", **T2)
check("one-off paid", mb["summary"]["one_off_status"] == "paid" and float(mb["summary"]["one_off_amount"]) == 50000, mb["summary"])
dash = call("GET", "/platform/billing")
check("billing dashboard: statuses, revenue, maintenance due", all(k in dash["counts"] for k in ("active", "deactivated"))
      and all(k in dash["by_status"] for k in ("active", "one_off_paid", "trial", "free", "payment_due", "maintenance_due", "grace", "overdue", "suspended"))
      and float(dash["revenue"]["one_off"]["all"]) >= 50000 and dash["maintenance_due"]["count"] >= 1
      and any(r["id"] == dk for r in dash["items"]), (dash["counts"], dash["revenue"], dash["maintenance_due"]))
check("billing activity in the platform activity view", call("GET", f"/platform/activity?activity=billing&tenant_id={dk}&period=today")["total"] >= 3)

step("Roadmap 36: deactivate / reactivate a business")
call("POST", f"/platform/tenants/{dk}/status", {"status": "deactivated", "reason": ""}, expect=422)
check("deactivation needs a reason", True)
call("POST", f"/platform/tenants/{home_id}/status", {"status": "deactivated", "reason": "Testing self lock-out"}, expect=422)
check("the platform owner's own business cannot be deactivated", True)
old_t2 = t2["token"]
time.sleep(1.1)
call("POST", f"/platform/tenants/{dk}/status", {"status": "deactivated", "reason": "Invoices unpaid"})
call("GET", "/auth/me", token=old_t2, branch="none", expect=401)
check("deactivation ends the business's sessions", True)
r = call("POST", "/auth/login", {"email": dk_email, "pin": rp["temporary_pin"]}, expect=422)
check("sign-in blocked with a clear title", r["error"]["title"] == "Business deactivated", r)
r = call("GET", f"/portal/{dk_slug}", token="none", expect=422)
check("ordering link disabled", r["error"]["title"] == "Ordering unavailable", r)
acting = call("POST", f"/platform/tenants/{dk}/open")
call("GET", "/dashboard?period=today", token=acting["token"], branch="none")
r = call("POST", "/categories", {"name": "Blocked"}, token=acting["token"], branch="none", expect=422)
check("platform owner can look inside but not transact", r["error"]["title"] == "Business deactivated", r)
dk_row = next(x for x in call("GET", "/platform/tenants")["items"] if x["id"] == dk)
check("status and reason in the directory", dk_row["status"] == "deactivated" and dk_row["status_reason"] == "Invoices unpaid", dk_row["status"])
check("data kept while deactivated", len(call("GET", f"/platform/tenants/{dk}")["billing"]["documents"]) >= 4)
call("POST", f"/platform/tenants/{dk}/status", {"status": "active", "reason": "Paid up"})
t2 = call("POST", "/auth/login", {"email": dk_email, "pin": rp["temporary_pin"]})
call("GET", "/auth/me", token=old_t2, branch="none", expect=401)
check("reactivated: sign-in works, sessions from before stay ended", bool(t2["token"]))
hist = call("GET", f"/platform/tenants/{dk}")["status_history"]
check("status history audited", [h["action"] for h in hist][:2] == ["reactivate_business", "deactivate_business"], hist)
check("platform audit trail at home", sum(1 for a in call("GET", "/audit?period=today&module=platform")["items"]
                                         if a["action"] in ("deactivate_business", "reactivate_business")) >= 2)

step("Roadmap 41–46: packages, tenant pricing, trial / free / grace, platform-owned business")
T2 = {"token": t2["token"], "branch": "none"}  # the session from after reactivation
home_row = next(x for x in call("GET", "/platform/tenants")["items"] if x["id"] == home_id)
check("the platform owner's business is platform owned", home_row["ownership"] == "platform" and home_row["billing"]["status"] == "platform_owned", home_row["billing"]["status"])
r = call("PUT", f"/platform/tenants/{home_id}/billing-plan", {"model": "subscription", "amount": 1000, "start_date": today_}, expect=422)
check("no billing plan for the platform owner's business", r["error"]["title"] == "Not billable", r)
call("POST", f"/platform/tenants/{home_id}/billing-documents", {"kind": "invoice", "category": "other", "amount": 100, "description": "Should not exist"}, expect=422)
check("no invoices for the platform owner's business", True)
check("platform owner keeps every module", call("GET", "/auth/me")["billing"]["modules"] is None)
later = (datetime.date.today() + datetime.timedelta(days=60)).isoformat()
plan_ = {"model": "subscription", "package": "modules", "modules": ["sales", "stock", "reports"],
         "module_prices": {"sales": 1000, "stock": 800, "reports": 700, "credit": 999}, "frequency": "monthly",
         "start_date": later, "next_due_date": later, "discount_type": "percent", "discount_value": 10, "tax_enabled": True, "tax_rate": 16}
call("PUT", f"/platform/tenants/{dk}/billing-plan", {**plan_, "modules": []}, expect=400)
check("a module package needs modules", True)
call("PUT", f"/platform/tenants/{dk}/billing-plan", {**plan_, "modules": ["sales", "teleport"]}, expect=400)
check("unknown module refused", True)
call("PUT", f"/platform/tenants/{dk}/billing-plan", {**plan_, "discount_value": 150}, expect=400)
check("percentage discount over 100% refused", True)
sp = call("PUT", f"/platform/tenants/{dk}/billing-plan", plan_)
rp_ = sp["summary"]["recurring_price"]
check("per-module price: base = included modules only", float(sp["plan"]["amount"]) == 2500 and "credit" not in sp["plan"]["module_prices"], sp["plan"])
check("base → discount → tax → payable", [float(rp_[k]) for k in ("subtotal", "discount", "tax", "total")] == [2500, 250, 360, 2610], rp_)
pinv = call("POST", f"/platform/tenants/{dk}/billing-documents", {"kind": "invoice", "category": "next_period"})
pdoc = call("GET", f"/billing/documents/{pinv['id']}", **T2)["document"]
check("invoice carries the breakdown", [float(pdoc[k]) for k in ("subtotal", "discount", "tax", "amount")] == [2500, 250, 360, 2610], pdoc)
changes = next(a for a in call("GET", f"/platform/activity?activity=billing&tenant_id={dk}&period=today")["items"] if a["action"] == "plan_updated")["after"]["changes"]
check("plan change audited with previous and new values", changes["tax_rate"]["to"] in ("16", "16.00", 16) and "modules" in changes and "from" in changes["package"], list(changes))
me_dk = call("GET", "/auth/me", **T2)
check("profile lists the package's modules", me_dk["billing"]["modules"] == ["sales", "stock", "reports"], me_dk["billing"])
blocked = []
for path_, want in [("/credit", 422), ("/expenses", 422), ("/customers", 422), ("/loyalty/overview", 422), ("/orders", 422),
                    ("/sales?period=today", 200), ("/stock", 200), ("/products", 200), ("/reports", 200), ("/billing", 200)]:
    try:
        r = call("GET", path_, **T2, expect=want)
        if want == 422 and r["error"]["title"] != "Not in your package":
            blocked.append(f"{path_}: {r['error']}")
    except AssertionError as e:
        blocked.append(str(e))
check("modules outside the package refused by the server, included ones work", not blocked, blocked)
r = call("GET", f"/portal/{dk_slug}", token="none", expect=422)
check("ordering link off without the Orders module", r["error"]["title"] == "Ordering unavailable", r)
act_ = call("POST", f"/platform/tenants/{dk}/open")
call("GET", "/credit", token=act_["token"], branch="none")
check("platform owner inside the business is not restricted", True)
call("POST", f"/platform/billing/documents/{pinv['id']}/void", {"reason": "Package changed"})
trial_end = (datetime.date.today() + datetime.timedelta(days=14)).isoformat()
day_after = (datetime.date.today() + datetime.timedelta(days=15)).isoformat()
call("PUT", f"/platform/tenants/{dk}/billing-plan", {"model": "subscription", "amount": 3000, "frequency": "monthly", "start_date": today_,
                                                     "access_mode": "trial", "trial_start": today_}, expect=400)
check("a trial needs its dates", True)
tr = call("PUT", f"/platform/tenants/{dk}/billing-plan", {"model": "subscription", "amount": 3000, "frequency": "monthly", "start_date": later,
                                                          "access_mode": "trial", "trial_start": today_, "trial_end": trial_end,
                                                          "trial_modules": ["sales", "credit", "expenses"]})
check("trial: status, end date, billing starts the day after", tr["summary"]["status"] == "trial" and tr["summary"]["trial_end"] == trial_end
      and tr["plan"]["next_due_date"] == day_after, (tr["summary"]["status"], tr["plan"]["next_due_date"]))
call("GET", "/credit", **T2)
call("GET", "/reports", **T2, expect=422)
check("trial modules apply during the trial", call("GET", "/auth/me", **T2)["billing"]["modules"] == ["sales", "credit", "expenses"])
fr = call("PUT", f"/platform/tenants/{dk}/billing-plan", {"model": "subscription", "access_mode": "free"})
check("free access without a price, business stays active", fr["summary"]["status"] == "free" and call("GET", "/billing", **T2)["summary"]["status"] == "free", fr["summary"]["status"])
call("GET", "/credit", **T2)
check("free full package: every module", call("GET", "/auth/me", **T2)["billing"]["modules"] is None)
gr = call("PUT", f"/platform/tenants/{dk}/billing-plan", {"model": "subscription", "amount": 1000, "frequency": "monthly", "start_date": later,
                                                          "grace_days": 3, "grace_until": later, "auto_suspend": True})
check("grace extension and automatic suspension saved", gr["plan"]["grace_until"] == later and gr["plan"]["auto_suspend"] is True
      and gr["summary"]["suspended"] is False, gr["plan"])
oo2 = call("PUT", f"/platform/tenants/{dk}/billing-plan", {"model": "one_off", "one_off_amount": 80000, "one_off_paid_on": today_,
                                                           "tax_enabled": True, "tax_rate": 16})
call("POST", f"/platform/tenants/{dk}/billing-documents", {"kind": "invoice", "category": "next_period"}, expect=422)
check("one-off without maintenance never produces a recurring invoice", oo2["summary"]["status"] == "one_off_paid", oo2["summary"]["status"])
mt = call("PUT", f"/platform/tenants/{dk}/billing-plan", {"model": "one_off", "one_off_amount": 80000, "one_off_paid_on": today_,
                                                          "maintenance": True, "amount": 120000, "frequency": "annual", "start_date": later,
                                                          "tax_enabled": True, "tax_rate": 16})
check("annual maintenance 120,000 + 16% tax", float(mt["summary"]["recurring_price"]["total"]) == 139200 and mt["summary"]["one_off_status"] == "paid", mt["summary"]["recurring_price"])
call("PUT", f"/platform/tenants/{dk}/billing-plan", {"model": "subscription", "access_mode": "free"})

step("Roadmap 47: duplicate submissions refused by the server")
dup_body = {"name": f"Dup {suffix}"}
first_ = call("POST", "/categories", dup_body, repeat=True)
again_ = call("POST", "/categories", dup_body, repeat=True)
check("an immediate identical repeat returns the first result, nothing created twice", again_ == first_
      and sum(1 for c in call("GET", "/categories") if c["name"] == f"Dup {suffix}") == 1, (first_, again_))
import concurrent.futures as _cf
def _race():
    try:
        return call("POST", "/categories", {"name": f"Race {suffix}"}, repeat=True), 200
    except AssertionError as e:
        return str(e), int(str(e).split("→ ")[1].split(" ")[0])
with _cf.ThreadPoolExecutor(4) as ex:
    codes = sorted(r[1] for r in ex.map(lambda _: _race(), range(4)))
check("simultaneous duplicates: one runs, the rest are refused or answered with it", codes.count(200) >= 1
      and all(c in (200, 409) for c in codes) and sum(1 for c in call("GET", "/categories") if c["name"] == f"Race {suffix}") == 1, codes)
call("POST", "/categories", {"name": ""}, expect=400, repeat=True)
call("POST", "/categories", {"name": ""}, expect=400, repeat=True)
check("a failed request is not remembered: retrying runs it again", True)

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
