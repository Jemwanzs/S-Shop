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
import base64
import json
import re
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


def call(method, path, body=None, expect=200, token=None, branch=None, location=None, repeat=False, activity=True):
    req = urllib.request.Request(BASE + "/api" + path.replace(" ", "%20"), method=method)
    req.add_header("Content-Type", "application/json")
    # The suite acts as a person at the keyboard (roadmap 87): requests count as activity unless a test says otherwise.
    if activity:
        req.add_header("X-User-Activity", "1")
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
# Resend stand-in (roadmap 58–61): the server is started with RESEND_BASE_URL pointing here, so every email it sends is
# captured and the suite can follow the links inside.
RS_KEY = os.environ.get("SMOKE_RESEND_KEY", "")
RS_WEBHOOK = os.environ.get("SMOKE_RESEND_WEBHOOK_SECRET", "")
EMAILS = []
# Daraja stand-in (roadmap 91): the server is started with MPESA_SANDBOX_URL pointing here. Only the consumer key and
# secret below get a token; a prompt is accepted only with a password built from the right passkey. Every accepted
# prompt is kept (with its callback URL) so the suite can answer as Safaricom would.
DARAJA = {"key": "ck_smoke_key_1234", "secret": "cs_smoke_secret_9f8e7d", "passkey": "pk_smoke_passkey_a1b2c3"}
STK = {}


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

    def _daraja(self):
        body = json.loads(self.rfile.read(int(self.headers.get("Content-Length", 0))) or b"{}")
        if not self.headers.get("Authorization", "").startswith("Bearer tok_"):
            return self._send(401, {"errorCode": "404.001.03", "errorMessage": "Invalid Access Token"})
        if self.path == "/mpesa/stkpush/v1/processrequest":
            pwd = base64.b64decode(body["Password"]).decode()
            if pwd != body["BusinessShortCode"] + DARAJA["passkey"] + body["Timestamp"]:
                return self._send(400, {"errorCode": "400.002.02", "errorMessage": "Bad Request - Invalid Password"})
            cid = "ws_CO_" + uuid.uuid4().hex[:20]
            STK[cid] = {"req": body, "query": None}
            return self._send(200, {"MerchantRequestID": "m-" + cid, "CheckoutRequestID": cid, "ResponseCode": "0",
                                    "ResponseDescription": "Success. Request accepted for processing", "CustomerMessage": "Success. Request accepted for processing"})
        if self.path == "/mpesa/stkpushquery/v1/query":
            q = STK.get(body.get("CheckoutRequestID"), {}).get("query")
            if q is None:
                return self._send(500, {"errorCode": "500.001.1001", "errorMessage": "The transaction is being processed"})
            return self._send(200, {"ResponseCode": "0", "ResultCode": str(q[0]), "ResultDesc": q[1]})
        self._send(404, {"errorMessage": "Not found"})

    def do_POST(self):
        if self.path.startswith("/mpesa/"):
            return self._daraja()
        if self.path == "/emails":
            if not RS_KEY or self.headers.get("Authorization") != "Bearer " + RS_KEY:
                return self._send(401, {"message": "API key is invalid"})
            mail = json.loads(self.rfile.read(int(self.headers.get("Content-Length", 0))) or b"{}")
            mail["id"] = "em_" + uuid.uuid4().hex
            EMAILS.append(mail)
            return self._send(200, {"id": mail["id"]})
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
        if self.path.startswith("/oauth/v1/generate"):
            ok = self.headers.get("Authorization") == "Basic " + base64.b64encode(f"{DARAJA['key']}:{DARAJA['secret']}".encode()).decode()
            if not ok:
                return self._send(400, {"errorCode": "400.008.01", "errorMessage": "Invalid Authentication passed"})
            return self._send(200, {"access_token": "tok_" + uuid.uuid4().hex, "expires_in": "3599"})
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


def make_xlsx(headers, rows, sheet="Data"):
    """A minimal .xlsx workbook (first sheet: headers on row 1, then rows; strings inline, numbers as numbers)."""
    import zipfile
    from xml.sax.saxutils import escape

    def col(i):
        s = ""
        i += 1
        while i:
            i, r = divmod(i - 1, 26)
            s = chr(65 + r) + s
        return s

    def cell(ref, v):
        if v is None or v == "":
            return ""
        if isinstance(v, (int, float)) and not isinstance(v, bool):
            return f'<c r="{ref}"><v>{v}</v></c>'
        return f'<c r="{ref}" t="inlineStr"><is><t>{escape(str(v))}</t></is></c>'

    xml_rows = []
    for r, values in enumerate([headers] + rows, start=1):
        xml_rows.append(f'<row r="{r}">' + "".join(cell(f"{col(c)}{r}", v) for c, v in enumerate(values)) + "</row>")
    sheet_xml = ('<?xml version="1.0" encoding="UTF-8" standalone="yes"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">'
                 "<sheetData>" + "".join(xml_rows) + "</sheetData></worksheet>")
    buf = io_bytes()
    with zipfile.ZipFile(buf, "w", zipfile.ZIP_DEFLATED) as z:
        z.writestr("[Content_Types].xml", '<?xml version="1.0" encoding="UTF-8"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">'
                   '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/>'
                   '<Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/>'
                   '<Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/></Types>')
        z.writestr("_rels/.rels", '<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
                   '<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>')
        z.writestr("xl/workbook.xml", '<?xml version="1.0" encoding="UTF-8"?><workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" '
                   'xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets>'
                   f'<sheet name="{sheet}" sheetId="1" r:id="rId1"/></sheets></workbook>')
        z.writestr("xl/_rels/workbook.xml.rels", '<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
                   '<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/></Relationships>')
        z.writestr("xl/worksheets/sheet1.xml", sheet_xml)
    return buf.getvalue()


def io_bytes():
    import io as _io
    return _io.BytesIO()



def import_now(batch_id, body=None, token=None):
    """Roadmap 103: posting runs in the background — submit, then follow the real progress until it settles."""
    r = call("POST", f"/onboarding/batches/{batch_id}/post", body or {}, token=token)
    if r["status"] != "posting":
        return r
    seen = []
    for _ in range(600):
        pr = call("GET", f"/onboarding/batches/{batch_id}/progress", token=token)
        seen.append((pr["stage"], pr["done"]))
        if pr["status"] != "posting":
            return {"status": pr["status"], "summary": pr["summary"], "progress": pr, "seen": seen}
        time.sleep(0.1)
    raise AssertionError(f"import {batch_id} still running: {seen[-3:]}")


def has_price_value(obj, *prices):
    """True when any value in the JSON equals one of the prices (number or numeric text) — exact values only, so
    digits that happen to appear inside an id or a slug are not mistaken for a leaked price."""
    want = {float(x) for x in prices}
    if isinstance(obj, dict):
        return any(has_price_value(v, *prices) for v in obj.values())
    if isinstance(obj, list):
        return any(has_price_value(v, *prices) for v in obj)
    if isinstance(obj, bool) or obj is None:
        return False
    if isinstance(obj, (int, float)):
        return float(obj) in want
    if isinstance(obj, str):
        try:
            return float(obj.replace(",", "")) in want
        except ValueError:
            return False
    return False

def mentions_word(obj, word):
    """`word` appears as a whole value or word (not as a run of characters inside an id, token or slug)."""
    return re.search(rf"(?<![0-9A-Za-z]){re.escape(str(word))}(?![0-9A-Za-z])", json.dumps(obj)) is not None


def upload_xlsx(dataset, data, mode="create", expect=200, token=None, name="import.xlsx"):
    """Multipart upload of a workbook to the onboarding centre."""
    boundary = "----sshop" + uuid.uuid4().hex
    parts = [f'--{boundary}\r\nContent-Disposition: form-data; name="mode"\r\n\r\n{mode}\r\n'.encode(),
             f'--{boundary}\r\nContent-Disposition: form-data; name="file"; filename="{name}"\r\nContent-Type: application/vnd.openxmlformats-officedocument.spreadsheetml.sheet\r\n\r\n'.encode()
             + data + b"\r\n", f"--{boundary}--\r\n".encode()]
    req = urllib.request.Request(BASE + f"/api/onboarding/{dataset}/upload", data=b"".join(parts), method="POST")
    req.add_header("Content-Type", f"multipart/form-data; boundary={boundary}")
    req.add_header("Authorization", "Bearer " + (token or TOKEN))
    req.add_header("X-Branch-Id", BRANCH)
    req.add_header("X-User-Activity", "1")
    try:
        with urllib.request.urlopen(req) as res:
            status, body = res.status, json.loads(res.read() or b"null")
    except urllib.error.HTTPError as e:
        status, body = e.code, json.loads(e.read() or b"null")
    if expect is not None and status != expect:
        raise AssertionError(f"upload {dataset} → {status} (expected {expect}): {body}")
    return body


def mails_to(address, subject_part=""):
    """Emails the server sent to this address (newest last), waiting briefly for background sends."""
    for _ in range(40):
        got = [m for m in EMAILS if address in m.get("to", []) and subject_part in m.get("subject", "")]
        if got:
            return got
        time.sleep(0.25)
    return []


def link_token(mail):
    m = re.search(r"#t=([A-Za-z0-9_-]+)", mail.get("text", ""))
    return m.group(1) if m else None


def support_open(tid, scope="full"):
    """Roadmap 71: the platform owner enters another business through a support session (PIN, reason, time limit)."""
    return call("POST", f"/platform/tenants/{tid}/support", {"pin": PIN, "reason": "Smoke test: platform support check", "scope": scope, "minutes": 60})


def first_login(email, temp_pin, new_pin):
    """Sign in with a one-time PIN, replace it (required before anything else) and sign in with the new PIN."""
    t = call("POST", "/auth/login", {"email": email, "pin": temp_pin})
    call("POST", "/auth/change-pin", {"current_pin": temp_pin, "new_pin": new_pin}, token=t["token"], branch="none")
    return call("POST", "/auth/login", {"email": email, "pin": new_pin})


def resend_webhook(event, sign=True):
    raw = json.dumps(event).encode()
    sid, ts = "msg_" + uuid.uuid4().hex, str(int(time.time()))
    key = base64.b64decode(RS_WEBHOOK.removeprefix("whsec_"))
    sig = base64.b64encode(hmac.new(key, f"{sid}.{ts}.".encode() + raw, hashlib.sha256).digest()).decode() if sign else "AAAA"
    req = urllib.request.Request(BASE + "/api/webhooks/resend", data=raw, method="POST")
    req.add_header("Content-Type", "application/json")
    req.add_header("svix-id", sid)
    req.add_header("svix-timestamp", ts)
    req.add_header("svix-signature", "v1," + sig)
    try:
        with urllib.request.urlopen(req) as r:
            return r.status
    except urllib.error.HTTPError as e:
        return e.code


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
check("signed-in profile carries the inactivity limits", login["profile"]["session"]["idle_seconds"] >= 60 and login["profile"]["session"]["warning_seconds"] >= 10)
# Sessions of businesses created during the run sit unused while other sections run: give them room (roadmap 87 is
# tested on its own, with short limits).
_sec = call("GET", "/platform/security")
call("PUT", "/platform/security", {**_sec, "idle_default_minutes": 120, "idle_max_minutes": max(_sec["idle_max_minutes"], 120)})
# An open session takes a longer limit at its next heartbeat (the app sends one after real input).
check("heartbeat applies the new limit to this session", call("POST", "/auth/activity", {})["idle_seconds"] == 7200)
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
t2 = first_login(req["email"], ap["temporary_pin"], "dk2468")
DK_PIN = "dk2468"
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
check("preferences saved on the profile", me["user"]["preferences"] == {"language": "en", "font": "Poppins", "currency": "USD", "quick_sale_draggable": False,
                                                                "notify_new_orders": True, "in_app_alerts": True, "sound_alerts": False, "landing": ""}, me["user"]["preferences"])
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
r = call("POST", f"/platform/tenants/{other_t['id']}/open", expect=422)
check("another business never opens directly", r["error"]["title"] == "Support access needed", r)
op = support_open(other_t["id"])
check("opened business profile shows acting", op["profile"]["tenant"]["id"] == other_t["id"] and op["profile"]["acting"]["home_tenant_id"] == login["profile"]["tenant"]["id"], op["profile"].get("acting"))
acting = op["token"]
other_branch = op["profile"]["branches"][0]["id"]
me_act = call("GET", "/auth/me", token=acting, branch=other_branch)
check("full access inside the opened business", "*" in me_act["permissions"] and me_act["tenant"]["id"] == other_t["id"])
check("acting session sees only that business's data", all(x["id"] != nduma for x in call("GET", "/products?status=all", token=acting, branch=other_branch)["items"]))
audit_o = call("GET", "/audit?period=today&limit=200", token=acting, branch=other_branch)["items"]
check("support access is in that business's audit trail", any(x.get("action") == "support_started" for x in audit_o))
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
rep_staff = call("GET", "/leaderboards/staff?period=today", token=reporter)["items"]
check("without “view other employees” the staff board shows only the user's own row (roadmap 64)", len(rep_staff) <= 1, [x["name"] for x in rep_staff])
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
def upload(pid, data, ref=None, expect=200, thumb=None):
    boundary = "sshop" + uuid.uuid4().hex
    parts = []
    if ref:
        parts.append(f'--{boundary}\r\nContent-Disposition: form-data; name="upload_ref"\r\n\r\n{ref}\r\n'.encode())
    parts.append(f'--{boundary}\r\nContent-Disposition: form-data; name="file"; filename="p.png"\r\nContent-Type: image/png\r\n\r\n'.encode() + data + b"\r\n")
    if thumb:
        parts.append(f'--{boundary}\r\nContent-Disposition: form-data; name="thumb"; filename="t.png"\r\nContent-Type: image/png\r\n\r\n'.encode() + thumb + b"\r\n")
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
# Roadmap 75: a small copy for grids; photos without one serve the original.
THUMB = b"\x89PNG\r\n\x1a\n" + b"\x01" * 300
tp_prod = call("POST", "/products", {"name": f"Thumb Test {suffix}", "marked_price": 100})["result"]["id"]
tp = upload(tp_prod, PNG, str(uuid.uuid4()), thumb=THUMB)
check("photo thumbnail stored and served with ?size=thumb", call("GET", f"/photos/{tp['id']}?size=thumb") == THUMB and call("GET", f"/photos/{tp['id']}") == PNG)
check("older photos without a thumbnail serve the original", call("GET", f"/photos/{first['id']}?size=thumb") == PNG)
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
# Roadmap 88: the Product User Manual downloads from the sign-in screen without signing in (a PDF, not the app page).
_man = urllib.request.urlopen(BASE + "/SShop_Product_User_Manual.pdf")
_body = _man.read()
check("user manual is public and a PDF", _man.status == 200 and _body[:5] == b"%PDF-" and "pdf" in _man.headers.get("Content-Type", ""), _man.headers.get("Content-Type"))

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
logins = call("GET", f"/platform/activity?activity=login&tenant_id={dk}&user_id={dk_admin}&period=week")
check("sign-ins filtered by business and user", logins["total"] >= 1 and all(i["user_id"] == dk_admin for i in logins["items"]), logins["total"])
sales_act = call("GET", f"/platform/activity?activity=sale&tenant_id={home_id}&period=today")
check("sales activity across businesses", sales_act["totals"]["sale"] >= 1 and sales_act["totals"]["login"] == 0, sales_act["totals"])
call("GET", "/platform/activity?activity=nonsense", expect=400)
rp = call("POST", f"/platform/tenants/{dk}/users/{dk_admin}/reset-pin")
call("POST", "/auth/login", {"email": dk_email, "pin": DK_PIN}, expect=400)
t2 = first_login(dk_email, rp["temporary_pin"], "dk1357")
DK_PIN = "dk1357"
check("platform PIN reset: old PIN refused, one-time PIN works once and is replaced", t2["profile"]["tenant"]["id"] == dk
      and t2["profile"]["user"]["must_change_pin"] is False)
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
r = call("POST", "/auth/login", {"email": dk_email, "pin": DK_PIN}, expect=422)
check("sign-in blocked with a clear title", r["error"]["title"] == "Business deactivated", r)
r = call("GET", f"/portal/{dk_slug}", token="none", expect=422)
check("ordering link disabled", r["error"]["title"] == "Ordering unavailable", r)
acting = support_open(dk)
call("GET", "/dashboard?period=today", token=acting["token"], branch="none")
r = call("POST", "/categories", {"name": "Blocked"}, token=acting["token"], branch="none", expect=422)
check("platform owner can look inside but not transact", r["error"]["title"] == "Business deactivated", r)
dk_row = next(x for x in call("GET", "/platform/tenants")["items"] if x["id"] == dk)
check("status and reason in the directory", dk_row["status"] == "deactivated" and dk_row["status_reason"] == "Invoices unpaid", dk_row["status"])
check("data kept while deactivated", len(call("GET", f"/platform/tenants/{dk}")["billing"]["documents"]) >= 4)
call("POST", f"/platform/tenants/{dk}/status", {"status": "active", "reason": "Paid up"})
t2 = call("POST", "/auth/login", {"email": dk_email, "pin": DK_PIN})
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
act_ = support_open(dk)
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
# Recurring invoices left open by the earlier plans are voided first (as an owner would when changing the model).
for d_ in call("GET", f"/platform/tenants/{dk}")["billing"]["documents"]:
    if d_["status"] == "open":
        call("POST", f"/platform/billing/documents/{d_['id']}/void", {"reason": "Model changed"})
oo2 = call("PUT", f"/platform/tenants/{dk}/billing-plan", {"model": "one_off", "one_off_amount": 80000, "one_off_paid_on": today_,
                                                           "tax_enabled": True, "tax_rate": 16})
call("POST", f"/platform/tenants/{dk}/billing-documents", {"kind": "invoice", "category": "next_period"}, expect=422)
check("one-off without maintenance never produces a recurring invoice", oo2["summary"]["status"] == "one_off_paid", oo2["summary"]["status"])
mt = call("PUT", f"/platform/tenants/{dk}/billing-plan", {"model": "one_off", "one_off_amount": 80000, "one_off_paid_on": today_,
                                                          "maintenance": True, "amount": 120000, "frequency": "annual", "start_date": later,
                                                          "tax_enabled": True, "tax_rate": 16})
sub_inv = call("POST", f"/platform/tenants/{dk}/billing-documents", {"kind": "invoice", "category": "next_period"})
stale = call("PUT", f"/platform/tenants/{dk}/billing-plan", {"model": "one_off", "one_off_amount": 80000, "one_off_paid_on": today_})
check("maintenance switched off: open maintenance invoice reported, not deleted", stale["stale_invoices"] == 1
      and next(d for d in call("GET", f"/platform/tenants/{dk}")["billing"]["documents"] if d["id"] == sub_inv["id"])["status"] == "open", stale.get("stale_invoices"))
call("POST", f"/platform/billing/documents/{sub_inv['id']}/void", {"reason": "Maintenance dropped"})
check("annual maintenance 120,000 + 16% tax", float(mt["summary"]["recurring_price"]["total"]) == 139200 and mt["summary"]["one_off_status"] == "paid", mt["summary"]["recurring_price"])
call("PUT", f"/platform/tenants/{dk}/billing-plan", {"model": "subscription", "access_mode": "free"})

step("Roadmap 51–52: website service, management centre, draft → publish")
T2 = {"token": t2["token"], "branch": "none"}
call("PUT", f"/platform/tenants/{dk}/billing-plan", {"model": "subscription", "access_mode": "free"})
ov = call("GET", "/website", **T2)
check("website service starts locked", ov["status"] == "none", ov.get("status"))
call("PUT", "/website/draft", {"config": {}}, **T2, expect=422)
check("nothing to edit before activation", True)
call("POST", "/website/request", {"message": "We want a shop website"}, **T2)
call("POST", "/website/request", {"message": "again"}, **T2, expect=422)
check("request sent once", call("GET", "/website", **T2)["status"] == "requested")
owner_notes = call("GET", "/notifications")["items"]
check("platform owner notified of the request", any(n["title"].startswith("Website request:") for n in owner_notes))
row = next(x for x in call("GET", "/platform/tenants")["items"] if x["id"] == dk)
check("request visible in the platform directory", row["website_status"] == "requested" and "shop website" in (row["website_request_message"] or ""), row["website_status"])
call("POST", f"/platform/tenants/{dk}/website", {"action": "activate"}, token=t2["token"], branch="none", expect=403)
check("a business cannot activate its own website", True)
call("POST", f"/platform/tenants/{dk}/website", {"action": "activate"})
ov = call("GET", "/website", **T2)
d = ov["draft"]
check("activated with a ready-made draft from the business's details", ov["status"] == "active" and d["brand"]["name"] == dk_name
      and len(d["testimonials"]["items"]) == 5 and all(x["sample"] and not x["published"] for x in d["testimonials"]["items"])
      and d["products"]["max_photos"] == 5 and d["products"]["grid"]["mobile"] == 2, d["brand"])
check("not live until published", ov["live"] is False and ov["has_unpublished"] is True)
bad_theme = {**d, "theme": {**d["theme"], "light": {**d["theme"]["light"], "text": "#FBFBFB"}}}
r = call("PUT", "/website/draft", {"config": bad_theme}, **T2, expect=400)
check("unreadable colours refused", "hard to read" in r["error"]["message"], r)
bad_link = {**d, "hero": {**d["hero"], "primary": {"label": "Go", "target": "javascript:alert(1)"}}}
call("PUT", "/website/draft", {"config": bad_link}, **T2, expect=400)
check("unsafe links refused", True)
check("animations default to subtle", d["theme"].get("motion") == "subtle", d["theme"].get("motion"))
check("Products is the default landing page; portrait whole-product photos; 10 per page", d.get("landing") == "products"
      and d["products"]["grid"]["ratio"] == "portrait" and d["products"]["grid"]["fit"] == "contain"
      and d["products"].get("pagination") is True and d["products"].get("per_page") == 10, (d.get("landing"), d["products"].get("per_page")))
call("PUT", "/website/draft", {"config": {**d, "products": {**d["products"], "per_page": 0}}}, **T2, expect=400)
call("PUT", "/website/draft", {"config": {**d, "landing": "checkout"}}, **T2, expect=400)
check("products per page and landing page validated", True)
call("PUT", "/website/draft", {"config": {**d, "theme": {**d["theme"], "motion": "wild"}}}, **T2, expect=400)
check("unknown animation intensity refused", True)
d2 = {**d, "hero": {**d["hero"], "headline": "Shop the Difference."}, "theme": {**d["theme"], "style": "elegant"}}
saved = call("PUT", "/website/draft", {"config": d2, "base_updated_at": ov["draft_updated_at"]}, **T2)
check("draft saved with the parts that changed", sorted(saved["changed"]) == ["hero", "theme"], saved)
call("PUT", "/website/draft", {"config": d, "base_updated_at": ov["draft_updated_at"]}, **T2, expect=422)
check("a stale editor cannot overwrite someone else's newer draft", True)
# Roadmap 90: a sample's demonstration text can never be shown as a genuine review.
d3 = {**d2, "testimonials": {**d2["testimonials"], "items": [{**x, "published": True, "sample": False} for x in d2["testimonials"]["items"]]}}
r = call("PUT", "/website/draft", {"config": d3}, **T2, expect=400)
check("sample text can never be published as a genuine review", "own words" in r["error"]["message"], r)
# Samples switched on for the preview are allowed (they never reach the public site — checked after publishing).
d4 = {**d2, "testimonials": {**d2["testimonials"], "items": [{**x, "published": True} for x in d2["testimonials"]["items"]]}}
call("PUT", "/website/draft", {"config": d4}, **T2)
pub = call("POST", "/website/publish", {"note": "First version"}, **T2)
ov = call("GET", "/website", **T2)
check("published: live, version 1, nothing pending", pub["version"] == 1 and ov["live"] and not ov["has_unpublished"] and ov["published_by"], ov.get("version"))
call("POST", "/website/publish", {}, **T2, expect=422)
check("nothing new to publish", True)
d5 = {**d4, "hero": {**d4["hero"], "headline": "Draft only"}}
call("PUT", "/website/draft", {"config": d5}, **T2)
check("unpublished changes are flagged", call("GET", "/website", **T2)["has_unpublished"] is True)
call("POST", "/website/discard", **T2)
ov = call("GET", "/website", **T2)
check("discard returns the draft to the published version", ov["draft"]["hero"]["headline"] == "Shop the Difference." and not ov["has_unpublished"])
d6 = {**ov["draft"], "hero": {**ov["draft"]["hero"], "headline": "Version two"}}
call("PUT", "/website/draft", {"config": d6}, **T2)
call("POST", "/website/publish", {}, **T2)
call("POST", "/website/versions/1/restore", **T2)
ov = call("GET", "/website", **T2)
vers = call("GET", "/website/versions", **T2)["items"]
check("rollback restores version 1 as a new publication", ov["version"] == 3 and ov["draft"]["hero"]["headline"] == "Shop the Difference."
      and vers[0]["note"] == "Restored version 1", [v["note"] for v in vers])
# One price setting shared with the ordering link
call("PUT", "/website/prices", {"show_prices": False}, **T2)
check("website price visibility is the ordering-link setting", call("GET", "/settings", **T2)["settings"]["orders"]["show_prices"] is False)
call("PUT", "/website/prices", {"show_prices": True}, **T2)
# Users & Access: website permissions only, never operational access
roles_dk = call("GET", "/roles", **T2)
sales_dk = next(r["id"] for r in roles_dk if r["name"] == "Salesperson")
web_mail = f"web{suffix.lower()}@sshop.test"
call("POST", "/users", {"name": "Web Editor", "email": web_mail, "pin": "4321", "role_id": sales_dk, "all_branches": True, "branch_ids": []}, **T2)
web_user = next(u for u in call("GET", "/website/access", **T2)["users"] if u["email"] == web_mail)
call("PUT", f"/website/access/{web_user['id']}", {"permissions": ["website.content", "stock.adjust"]}, **T2, expect=400)
check("only website permissions can be given individually", True)
call("PUT", f"/website/access/{web_user['id']}", {"permissions": ["website.view", "website.content"]}, **T2)
web_tok = call("POST", "/auth/login", {"email": web_mail, "pin": "4321"})["token"]
W = {"token": web_tok, "branch": "none"}
wd = call("GET", "/website", **W)["draft"]
call("PUT", "/website/draft", {"config": {**wd, "about": {**wd["about"], "intro": "Edited by the web editor"}}}, **W)
r = call("PUT", "/website/draft", {"config": {**wd, "theme": {**wd["theme"], "style": "bold"}}}, **W, expect=403)
check("content editor can edit content but not the design", True)
call("POST", "/website/publish", {}, **W, expect=403)
check("editing without permission to publish", True)
call("GET", "/expenses", **W, expect=403)
call("POST", "/stock/receive", {"product_id": str(uuid.uuid4()), "quantity": 1}, **W, expect=403)
check("website access gives no operational access", True)
# Website billing is its own service in the same engine
wplan = call("PUT", f"/platform/tenants/{dk}/billing-plan", {"service": "website", "model": "one_off", "one_off_amount": 30000, "maintenance": True,
                                                            "amount": 1500, "frequency": "monthly", "start_date": later, "discount_type": "fixed", "discount_value": 500})
check("website billed separately: one-off + maintenance with its own discount", wplan["plan"]["service"] == "website"
      and float(wplan["summary"]["recurring_price"]["total"]) == 1000, wplan["summary"].get("recurring_price"))
winv = call("POST", f"/platform/tenants/{dk}/billing-documents", {"kind": "invoice", "category": "one_off", "service": "website", "amount": 30000})
mb = call("GET", "/billing", **T2)
check("website invoice shown with its service; platform plan untouched", mb["plan"]["model"] == "subscription" and float(mb["website"]["summary"]["outstanding"]) == 29500
      and any(x["id"] == winv["id"] and x["service"] == "website" for x in mb["documents"]), mb["website"]["summary"])
call("POST", f"/platform/billing/documents/{winv['id']}/void", {"reason": "Test"})
call("POST", f"/platform/tenants/{dk}/website", {"action": "disable", "reason": ""}, expect=422)
call("POST", f"/platform/tenants/{dk}/website", {"action": "disable", "reason": "Subscription ended"})
ov = call("GET", "/website", **T2)
check("disabled: not live, configuration kept", ov["status"] == "disabled" and not ov["live"] and ov["version"] == 3)
call("PUT", "/website/draft", {"config": wd}, **T2, expect=422)
call("POST", f"/platform/tenants/{dk}/website", {"action": "activate"})
check("re-activated with everything intact", call("GET", "/website", **T2)["live"] is True)
# Another business cannot see or change this website
call("GET", "/website/versions")
mine = call("GET", "/website")
check("each business sees only its own website", mine["status"] != "active" or mine.get("slug") != dk_slug, mine.get("status"))

step("Roadmap 53–57: public website, prices, ordering, media, domain, analytics")
PUB = {"token": "none", "branch": "none"}
dk_branch = call("GET", "/branches", **T2)[0]["id"]
T2B = {"token": t2["token"], "branch": dk_branch}
wcat = call("POST", "/categories", {"name": f"Bags {suffix}"}, **T2B)["id"]
bag = call("POST", "/products", {"name": f"Leather Bag {suffix}", "marked_price": 4321, "cost_price": 2000, "category_id": wcat}, **T2B)["result"]["id"]
hat = call("POST", "/products", {"name": f"Straw Hat {suffix}", "marked_price": 777, "cost_price": 300, "category_id": wcat}, **T2B)["result"]["id"]
call("POST", "/stock/receive", {"product_id": bag, "quantity": 5, "cost_price": 2000}, **T2B)
call("POST", "/stock/receive", {"product_id": hat, "quantity": 5, "cost_price": 300}, **T2B)
site = call("GET", f"/site?slug={dk_slug}", **PUB)
check("public website served by slug with business, categories and ordering", site["available"] and site["business"]["name"] == dk_name
      and any(c["id"] == wcat for c in site["categories"]) and site["ordering"]["enabled"], site.get("ordering"))
check("sample testimonials never reach the public (even switched on for the preview)", site["config"]["testimonials"]["items"] == [], site["config"]["testimonials"]["items"][:1])
check("internal product settings not exposed", not site["config"]["products"].get("items"))
plist = call("GET", f"/site/products?slug={dk_slug}", **PUB)["items"]
pbag = next(p for p in plist if p["id"] == bag)
check("new products appear automatically with price and slug", float(pbag["price"]) == 4321 and pbag["action"] == "add_to_cart"
      and pbag["slug"].startswith("leather-bag-"), pbag)
detail = call("GET", f"/site/products/{pbag['slug']}?slug={dk_slug}", **PUB)
check("product page by slug with related products", detail["product"]["id"] == bag and any(r["id"] == hat for r in detail["related"]))
sug = call("GET", f"/site/products?slug={dk_slug}&q=leath&suggest=true", **PUB)["items"]
check("search suggestions", [p["id"] for p in sug] == [bag], [p["name"] for p in sug])
# Roadmap 80: sorting by real data, paging counted in products, in-stock filter
ids = lambda q_: [p["id"] for p in call("GET", f"/site/products?slug={dk_slug}{q_}", **PUB)["items"]]
by_name = ids("&sort=name_desc")
by_price = ids("&sort=price_asc")
check("sort by name and by price", by_name.index(hat) < by_name.index(bag) and by_price.index(hat) < by_price.index(bag))
full = call("GET", f"/site/products?slug={dk_slug}&sort=name_asc", **PUB)
pg2 = call("GET", f"/site/products?slug={dk_slug}&sort=name_asc&limit=1&offset=1", **PUB)
check("pages: total stays the full count, one page of products", pg2["total"] == full["total"] and len(pg2["items"]) == 1
      and pg2["items"][0]["id"] == full["items"][1]["id"], (pg2["total"], full["total"]))
nostock = call("POST", "/products", {"name": f"Empty Shelf {suffix}", "marked_price": 50, "cost_price": 10, "category_id": wcat}, **T2B)["result"]["id"]
check("in-stock filter uses real stock", nostock not in ids("&stock=in") and bag in ids("&stock=in"))
# Hidden prices never leak — list, suggestions, detail, site data
call("PUT", "/website/prices", {"show_prices": False}, **T2)
site_all = [call("GET", f"/site/products?slug={dk_slug}", **PUB), call("GET", f"/site/products?slug={dk_slug}&q=leath&suggest=true", **PUB),
            call("GET", f"/site/products/{pbag['slug']}?slug={dk_slug}", **PUB), call("GET", f"/site?slug={dk_slug}", **PUB)]
check("hidden prices never leak anywhere", not has_price_value(site_all, 4321, 777))
hid = next(p for p in call("GET", f"/site/products?slug={dk_slug}", **PUB)["items"] if p["id"] == bag)
check("hidden price: no price, business's chosen action", hid["price"] is None and hid["action"] in ("enquire", "contact", "whatsapp", "add_to_cart"), hid)
# Per-product overrides (draft → publish)
ov = call("GET", "/website", **T2)
dd = ov["draft"]
base = {"published": True, "featured": False, "sort": 0, "marketing_name": "", "marketing_description": "", "badge": "", "price": "inherit",
        "hidden_action": "", "use_product_photos": True, "photos": [], "hidden_photos": [], "seo_title": "", "seo_description": "",
        "category_id": None, "cta_label": ""}
dd = {**dd, "products": {**dd["products"], "items": [
    {**base, "product_id": hat, "price": "show", "featured": True, "badge": "offer", "marketing_name": "Summer Straw Hat", "compare_at": "999"},
    {**base, "product_id": bag, "hidden_action": "order", "compare_at": "5000"}]}}
call("PUT", "/website/draft", {"config": dd}, **T2)
pre = next(p for p in call("GET", f"/site/products?slug={dk_slug}", **PUB)["items"] if p["id"] == hat)
check("draft changes are not public before publishing", pre["name"] != "Summer Straw Hat")
call("POST", "/website/publish", {}, **T2)
items = {p["id"]: p for p in call("GET", f"/site/products?slug={dk_slug}", **PUB)["items"]}
check("per-product: price shown on one product while hidden website-wide", float(items[hat]["price"]) == 777 and items[hat]["name"] == "Summer Straw Hat")
check("per-product: hidden price can still be ordered when allowed", items[bag]["price"] is None and items[bag]["action"] == "add_to_cart", items[bag])
check("was price shown only with a visible, lower price", float(items[hat]["compare_at"]) == 999 and items[bag]["compare_at"] is None
      and not has_price_value(items[bag], 5000), (items[hat].get("compare_at"), items[bag].get("compare_at")))
feat = call("GET", f"/site/products?slug={dk_slug}&section=featured", **PUB)["items"]
check("featured section", [p["id"] for p in feat] == [hat])
# Ordering through the website → existing orders engine
wmobile = "07" + str(uuid.uuid4().int)[:8]
call("POST", "/site/identify", {"slug": dk_slug, "mobile": wmobile}, **PUB)
wsess = call("POST", "/site/session", {"slug": dk_slug, "mobile": wmobile, "first_name": "Wanjiru"}, **PUB)
WC = {"token": wsess["token"], "branch": "none"}
vis = "v" + uuid.uuid4().hex[:12]
for kind, pid in [("visit", None), ("product_view", bag), ("product_view", bag), ("add_to_cart", bag), ("order_start", None)]:
    call("POST", "/site/events", {"slug": dk_slug, "kind": kind, "visitor": vis, "product_id": pid}, **PUB)
call("POST", "/site/events", {"slug": dk_slug, "kind": "order_complete", "visitor": vis}, **PUB, expect=400)
check("visitors cannot fake completed orders", True)
call("POST", "/site/orders", {"slug": dk_slug, "items": [{"product_id": bag, "quantity": 1}], "delivery_location": "Ngong Rd"}, **PUB, expect=401)
check("ordering needs a customer session", True)
wo = call("POST", "/site/orders", {"slug": dk_slug, "items": [{"product_id": bag, "quantity": 1}], "delivery_location": "Ngong Rd", "visitor": vis}, **WC)
check("website order placed; hidden total not revealed", wo["order_no"].startswith("ORD-") and wo["total"] is None and wo["price_note"], wo)
dk_items = call("GET", "/orders", **T2B)["items"]
check("order lands in Orders with source website", any(o["id"] == wo["id"] and o.get("source") == "website" for o in dk_items),
      [o.get("source") for o in dk_items[:3]])
call("POST", "/site/orders", {"slug": dk_slug, "items": [{"product_id": str(uuid.uuid4()), "quantity": 1}], "delivery_location": "X"}, **WC, expect=400)
check("unknown or unpublished products cannot be ordered", True)
home_slug = call("GET", "/auth/me")["tenant"]["slug"]
try:
    call("POST", "/site/orders", {"slug": home_slug, "items": [{"product_id": bag, "quantity": 1}], "delivery_location": "X"}, **WC)
    cross = False
except AssertionError as e:
    cross = any(f"→ {c}" in str(e) for c in (401, 404, 422))
check("a customer session cannot order from another business's website", cross)
# Analytics reconcile with orders
an = call("GET", "/website/analytics?period=today", **T2)
check("analytics: visitors, views, carts, starts, orders", an["visitors"] >= 1 and an["product_views"] >= 2 and an["add_to_carts"] >= 1
      and an["order_starts"] >= 1 and an["orders"] >= 1 and an["conversion"] > 0, {k: an[k] for k in ("visitors", "product_views", "orders")})
check("most viewed products", an["top_products"] and an["top_products"][0]["id"] == bag, an["top_products"][:1])
# Roadmap 84–86: holiday & promotional campaigns
import datetime as _cdt
def cl(days, hours=0):
    return (_cdt.datetime.now() + _cdt.timedelta(days=days, hours=hours)).strftime("%Y-%m-%dT%H:%M")
cg0 = call("GET", "/website/campaigns", **T2)
check("campaigns off by default, none yet", cg0["enabled"] is False and cg0["items"] == [] and cg0["platform"]["enabled"])
camp = {"name": "Christmas", "occasion": "christmas", "starts_local": cl(-1), "ends_local": cl(10), "priority": 0,
        "design": {"template": "christmas", "headline": "Merry Christmas!", "message": "Festive favourites", "cta_label": "Shop now", "cta_target": "products"},
        "products": {"mode": "manual", "product_ids": [hat], "limit": 5, "period_days": 90}, "placement": {"pages": ["products", "home"], "display": "hero", "dismissible": True}}
call("POST", "/website/campaigns", {**camp, "occasion": "birthday-party"}, **T2, expect=400)
call("POST", "/website/campaigns", {**camp, "ends_local": cl(-2)}, **T2, expect=400)
call("POST", "/website/campaigns", {**camp, "products": {**camp["products"], "product_ids": []}}, **T2, expect=400)
call("POST", "/website/campaigns", {**camp, "products": {**camp["products"], "product_ids": [nduma]}}, **T2, expect=400)
call("POST", "/website/campaigns", {**camp, "design": {**camp["design"], "cta_target": "javascript:alert(1)"}}, **T2, expect=400)
call("POST", "/website/campaigns", {**camp, "design": {**camp["design"], "text_color": "red"}}, **T2, expect=400)
check("campaigns validated: occasion, dates, products of this business only, safe links, colours", True)
c1 = call("POST", "/website/campaigns", camp, **T2)["id"]
check("saved as a draft: not on the website", call("GET", f"/site?slug={dk_slug}", **PUB)["campaign"] is None)
call("POST", f"/website/campaigns/{c1}/publish", {}, **W, expect=403)
call("POST", f"/website/campaigns/{c1}/publish", {}, **T2)
check("published but the website's switch is off: still hidden", call("GET", f"/site?slug={dk_slug}", **PUB)["campaign"] is None)
call("PUT", "/website/campaigns/settings", {"enabled": True}, **T2)
live = call("GET", f"/site?slug={dk_slug}", **PUB)["campaign"]
check("live campaign on the website with its featured product (website prices)", live and live["id"] == c1 and [x["id"] for x in live["products"]] == [hat]
      and float(live["products"][0]["price"]) == 777 and live["design"]["headline"] == "Merry Christmas!", live and live.get("id"))
# Priority, schedule, expiry
c2 = call("POST", "/website/campaigns", {**camp, "name": "Flash sale", "occasion": "black_friday", "priority": 5, "design": {**camp["design"], "template": "sale", "headline": "Flash sale"},
                                         "products": {"mode": "none", "product_ids": [], "limit": 3, "period_days": 90}}, **T2)["id"]
call("POST", f"/website/campaigns/{c2}/publish", {}, **T2)
check("overlapping campaigns: the higher priority shows, one at a time", call("GET", f"/site?slug={dk_slug}", **PUB)["campaign"]["id"] == c2)
c2v = next(x for x in call("GET", "/website/campaigns", **T2)["items"] if x["id"] == c2)["version"]
call("PUT", f"/website/campaigns/{c2}", {**camp, "name": "Flash sale", "occasion": "black_friday", "priority": 5, "starts_local": cl(3), "ends_local": cl(5),
                                          "design": {**camp["design"], "template": "sale", "headline": "Flash sale"}, "products": {"mode": "none", "product_ids": [], "limit": 3, "period_days": 90}, "version": c2v}, **T2)
items = {x["id"]: x for x in call("GET", "/website/campaigns", **T2)["items"]}
check("scheduled for later: not shown until it starts", items[c2]["status"] == "scheduled" and call("GET", f"/site?slug={dk_slug}", **PUB)["campaign"]["id"] == c1)
call("PUT", f"/website/campaigns/{c2}", {**camp, "version": c2v}, **T2, expect=422)
check("stale editors cannot overwrite a newer save", True)
# Insights and attribution
cvis = "v" + uuid.uuid4().hex[:12]
call("POST", "/site/events", {"slug": dk_slug, "kind": "campaign_view", "visitor": cvis, "campaign_id": c1}, **PUB)
call("POST", "/site/events", {"slug": dk_slug, "kind": "campaign_product", "visitor": cvis, "campaign_id": c1, "product_id": hat}, **PUB)
call("POST", "/site/events", {"slug": dk_slug, "kind": "campaign_cta", "visitor": cvis, "campaign_id": str(uuid.uuid4())}, **PUB, expect=400)
check("campaign events only for this business's campaigns", True)
call("POST", "/site/orders", {"slug": dk_slug, "items": [{"product_id": hat, "quantity": 1}], "delivery_location": "Ngong Rd", "visitor": cvis}, **WC)
ins = next(x for x in call("GET", "/website/campaigns", **T2)["items"] if x["id"] == c1)["insights"]
check("insights: views, product clicks, attributed order and sales", ins["views"] >= 1 and ins["product_clicks"] >= 1 and ins["orders"] == 1 and float(ins["sales"]) == 777, ins)
# Lifecycle
call("DELETE", f"/website/campaigns/{c1}", **T2, expect=422)
check("a published campaign is archived, not deleted", True)
dup = call("POST", f"/website/campaigns/{c1}/duplicate", {}, **T2)["id"]
call("DELETE", f"/website/campaigns/{dup}", **T2)
call("POST", f"/website/campaigns/{c1}/archive", {}, **T2)
call("POST", f"/website/campaigns/{c1}/publish", {}, **T2, expect=422)
check("duplicate, delete a draft, archive; archived cannot be published", call("GET", f"/site?slug={dk_slug}", **PUB)["campaign"] is None)
# Best sellers from real sales only
c3 = call("POST", "/website/campaigns", {**camp, "name": "Best sellers", "products": {"mode": "auto", "product_ids": [], "limit": 3, "period_days": 30}}, **T2)["id"]
call("POST", f"/website/campaigns/{c3}/publish", {}, **T2)
best = call("GET", f"/site?slug={dk_slug}", **PUB)["campaign"]
pub_ids = {x["id"] for x in call("GET", f"/site/products?slug={dk_slug}", **PUB)["items"]}
check("best sellers: published products only, never invented, within the limit", best["id"] == c3 and len(best["products"]) <= 3 and all(x["id"] in pub_ids for x in best["products"]))
# Platform switch wins
ps_c = call("GET", "/platform/campaigns")
call("PUT", "/platform/campaigns", {**ps_c, "enabled": False})
check("platform switch hides every campaign", call("GET", f"/site?slug={dk_slug}", **PUB)["campaign"] is None)
call("PUT", "/website/campaigns/settings", {"enabled": True}, **T2, expect=422)
call("PUT", "/platform/campaigns", ps_c)
call("PUT", "/platform/campaigns", {**ps_c, "max_products": 99}, expect=400)
call("GET", "/platform/campaigns", **T2, expect=403)
check("platform rules: only the platform owner, bounded limits", True)
call("GET", "/website/analytics?period=custom&from=2026-01-01&to=2025-01-01", **T2, expect=400)
call("GET", "/website/analytics", **W, expect=403)
check("analytics need their own permission", True)
# Media library: quality checks, tenant isolation
def wupload(kind, w, h, tok, expect=200, ref=None):
    png = b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR" + w.to_bytes(4, "big") + h.to_bytes(4, "big") + b"\x08\x06\x00\x00\x00" + b"\x00" * 4000
    boundary = "sshop" + uuid.uuid4().hex
    parts = [f'--{boundary}\r\nContent-Disposition: form-data; name="kind"\r\n\r\n{kind}\r\n'.encode()]
    if ref:
        parts.append(f'--{boundary}\r\nContent-Disposition: form-data; name="upload_ref"\r\n\r\n{ref}\r\n'.encode())
    parts.append(f'--{boundary}\r\nContent-Disposition: form-data; name="file"; filename="m.png"\r\nContent-Type: image/png\r\n\r\n'.encode() + png + b"\r\n")
    parts.append(f"--{boundary}--\r\n".encode())
    req = urllib.request.Request(BASE + "/api/website/media", data=b"".join(parts), method="POST")
    req.add_header("Authorization", "Bearer " + tok)
    req.add_header("Content-Type", f"multipart/form-data; boundary={boundary}")
    try:
        with urllib.request.urlopen(req) as r:
            status, raw = r.status, r.read()
    except urllib.error.HTTPError as e:
        status, raw = e.code, e.read()
    if status != expect:
        raise AssertionError(f"media upload → {status} (expected {expect}): {raw[:200]}")
    return json.loads(raw)
good = wupload("product", 1000, 1000, t2["token"])
check("✓ good image accepted", good["quality"] == "good", good)
mref = str(uuid.uuid4())
warn = wupload("banner", 600, 600, t2["token"], ref=mref)
check("⚠ low-resolution / square banner accepted with warnings", warn["quality"] == "warning" and len(warn["warnings"]) >= 2, warn)
check("retried upload stored once", wupload("banner", 600, 600, t2["token"], ref=mref)["id"] == warn["id"])
check("✕ tiny image refused", "too small" in wupload("product", 120, 120, t2["token"], expect=422)["error"]["message"])
dm = call("GET", "/website/media", **T2)["items"]
check("media library lists the business's images", {good["id"], warn["id"]} <= {m["id"] for m in dm})
call("PATCH", f"/website/media/{good['id']}", {"name": "Hacked"}, expect=404)
check("another business cannot touch this media", True)
dd = call("GET", "/website", **T2)["draft"]
dd = {**dd, "hero": {**dd["hero"], "image": good["id"]}}
call("PUT", "/website/draft", {"config": dd}, **T2)
call("DELETE", f"/website/media/{good['id']}", **T2, expect=422)
check("media in use cannot be deleted", True)
foreign = {**dd, "hero": {**dd["hero"], "image": str(uuid.uuid4())}}
call("PUT", "/website/draft", {"config": foreign}, **T2, expect=400)
check("only the business's own media can be used", True)
# Custom domain: guarded, unique, verification records
dom = f"shop{suffix.lower()}.example.com"
call("PUT", "/website/domain", {"domain": "x.up.railway.app"}, **T2, expect=400)
call("PUT", "/website/domain", {"domain": "localhost"}, **T2, expect=400)
check("platform and hosting domains refused", True)
dv = call("PUT", "/website/domain", {"domain": f"https://{dom.upper()}/about"}, **T2)["domain"]
check("domain saved, normalised, awaiting DNS with an ownership TXT record", dv["domain"] == dom and dv["status"] == "dns_required"
      and dv["records"][0]["fqdn"] == f"_sshop-verify.{dom}" and dv["records"][0]["name"] == f"_sshop-verify.shop{suffix.lower()}" and dv["records"][0]["value"].startswith("sshop-verify="), dv)
check("same domain again keeps its token", call("PUT", "/website/domain", {"domain": dom}, **T2)["domain"]["records"][0]["value"] == dv["records"][0]["value"])
check("the own domain is the main address by default; not shown as awaiting the platform before ownership", dv["is_primary"] is True and dv["awaiting_platform"] is False)
check("main address can be switched back to the S'Shop address", call("PUT", "/website/domain/primary", {"primary": False}, **T2)["domain"]["is_primary"] is False)
call("PUT", "/website/domain/primary", {"primary": True}, **W, expect=403)
call("PUT", "/website/domain/primary", {"primary": True}, **T2)
call("PUT", "/website/domain", {"domain": dom}, **W, expect=403)
check("connecting a domain needs website.domain", True)
home_t = call("GET", "/auth/me")["tenant"]["id"]
if call("GET", "/website")["status"] != "active":
    call("POST", f"/platform/tenants/{home_t}/website", {"action": "activate"})
r = call("PUT", "/website/domain", {"domain": dom}, expect=422)
check("a domain belongs to one business only", "another business" in r["error"]["message"], r)
req_h = urllib.request.Request(BASE + f"/api/site", method="GET")
req_h.add_header("Host", dom)
try:
    urllib.request.urlopen(req_h)
    unverified = False
except urllib.error.HTTPError as e:
    unverified = e.code in (400, 404)
check("an unverified domain serves nothing", unverified)
call("DELETE", "/website/domain", **T2)
check("domain removed; website stays on its S'Shop address", call("GET", "/website/domain", **T2)["domain"] is None
      and call("GET", "/website", **T2)["public_url"].endswith(f"/s/{dk_slug}"))
# A product's hidden price stays hidden everywhere: website, ordering link, order history, tracking
call("PUT", "/website/prices", {"show_prices": True}, **T2)
hd = call("GET", "/website", **T2)["draft"]
hd = {**hd, "products": {**hd["products"], "items": [{**x, "price": "hide"} if x["product_id"] == bag else x for x in hd["products"]["items"]]}}
call("PUT", "/website/draft", {"config": hd}, **T2)
call("POST", "/website/publish", {}, **T2)
wi = {p["id"]: p for p in call("GET", f"/site/products?slug={dk_slug}", **PUB)["items"]}
check("website: hidden product has no price while others show theirs", wi[bag]["price"] is None and float(wi[hat]["price"]) == 777)
pc = {p["id"]: p for p in call("GET", f"/portal/{dk_slug}/catalogue", **PUB)["products"]}
check("ordering link: the same product shows no price", bag in pc and "price" not in pc[bag] and float(pc[hat]["price"]) == 777, pc.get(bag))
check("ordering link product page: no price", "price" not in call("GET", f"/portal/{dk_slug}/products/{bag}", **PUB)["product"])
tr = call("GET", f"/portal/track/{wo['track_token']}", **PUB)
check("tracking an order with a hidden-price product shows no prices", not has_price_value(tr, 4321) and tr["business"]["show_prices"] is False)
mo = call("GET", f"/portal/{dk_slug}/orders", **WC)["orders"]
check("order history: no prices for that order", not has_price_value(mo, 4321), mo[:1])
po = call("POST", f"/portal/{dk_slug}/orders", {"items": [{"product_id": bag, "quantity": 1}], "delivery_location": "Shop"}, **WC)
check("ordering link confirmation: no total", "total" not in po, po)

step("Roadmap 58–61: onboarding emails, one-time PINs, self-service reset, applicant status")
ob_mail = f"ob{suffix.lower()}@sshop.test"
ob = {"business_name": f"Onboard {suffix}", "contact_name": "Libbie Bright", "email": ob_mail, "phone": "0722 333 444", "location": "Nairobi",
      "business_type": "Restaurant / café", "branches": 2, "estimated_users": 12, "message": ""}
call("POST", "/access-requests", {**ob, "estimated_users": 0}, token="none", expect=400)
check("estimated users validated", True)
call("POST", "/access-requests", ob, token="none")
owner = [m for m in mails_to(EMAIL, ob["business_name"]) if "access request" in m["subject"].lower()]
check("owner emailed with every detail incl. estimated users", owner and "Estimated users: 12" in owner[-1]["text"] and "0722 333 444" in owner[-1]["text"]
      and "<table" in owner[-1].get("html", ""), [m["subject"] for m in owner])
ack = mails_to(ob_mail, "We received")
check("applicant acknowledgement sent", len(ack) == 1, len(ack))
call("POST", "/access-requests", ob, token="none")
time.sleep(1)
check("a repeated request sends nothing again", len(mails_to(ob_mail, "We received")) == 1)
obr = next(x for x in call("GET", "/platform/access-requests?status=pending")["items"] if x["email"] == ob_mail)
check("request lists its email statuses", obr["emails"].get("access_request_ack", {}).get("status") == "sent" and obr["estimated_users"] == 12, obr.get("emails"))
# Approval: welcome email with a set-up link; the PIN is shown once and never emailed or put in the WhatsApp text
oap = call("POST", f"/platform/access-requests/{obr['id']}/approve")
welcome = mails_to(ob_mail, "Welcome to S'Shop")
check("approval: welcome email sent, status reported", oap["email_status"]["status"] == "sent" and len(welcome) == 1, oap["email_status"])
check("PIN never in the email or the WhatsApp message", oap["temporary_pin"] not in welcome[0]["text"] and oap["temporary_pin"] not in welcome[0]["html"]
      and oap["temporary_pin"] not in oap["message"] and "Welcome to S'Shop" in oap["message"])
check("WhatsApp recipient in international format", oap["wa_phone"] == "254722333444", oap["wa_phone"])
setup_tok = link_token(welcome[0])
info = call("POST", "/auth/link", {"token": setup_tok}, token="none")
check("set-up link: purpose and masked email only", info["kind"] == "setup" and "•••" in info["email"] and info["business"] == ob["business_name"], info)
tmp = call("POST", "/auth/login", {"email": ob_mail, "pin": oap["temporary_pin"]})
check("one-time PIN signs in but must be replaced", tmp["profile"]["user"]["must_change_pin"] is True)
r = call("GET", "/dashboard?period=today", token=tmp["token"], branch="none", expect=422)
check("nothing else works until the PIN is replaced", r["error"]["title"] == "Set your own PIN", r)
d = call("GET", f"/platform/access-requests/{obr['id']}")
check("login details: awaiting set-up, no PIN stored or shown", d["activation"]["status"] == "awaiting_setup" and oap["temporary_pin"] not in json.dumps(d)
      and any(e["kind"] == "welcome" and e["status"] == "sent" for e in d["emails"]), d["activation"])
time.sleep(1.1)
call("POST", "/auth/set-pin", {"token": setup_tok, "pin": "12"}, token="none", expect=400)
check("PIN policy applies", True)
done = call("POST", "/auth/set-pin", {"token": setup_tok, "pin": "ob5678"}, token="none")
check("set-up link sets the first PIN", done["email"] == ob_mail)
r = call("POST", "/auth/set-pin", {"token": setup_tok, "pin": "ob9999"}, token="none", expect=422)
check("a link works only once", r["error"]["title"] == "Link expired", r)
call("GET", "/auth/me", token=tmp["token"], branch="none", expect=401)
call("POST", "/auth/login", {"email": ob_mail, "pin": oap["temporary_pin"]}, expect=400)
check("old one-time PIN and its session stop working", True)
check("confirmation email after the change", len(mails_to(ob_mail, "PIN was changed")) == 1)
obt = call("POST", "/auth/login", {"email": ob_mail, "pin": "ob5678"})
check("signs in normally afterwards", obt["profile"]["user"]["must_change_pin"] is False)
check("activation: active", call("GET", f"/platform/access-requests/{obr['id']}")["activation"]["status"] == "active")
# Lost credentials: a fresh one-time PIN from the approved request card
call("POST", f"/platform/access-requests/{obr['id']}/issue-pin", token=obt["token"], branch="none", expect=403)
check("only the platform owner can issue PINs", True)
np_ = call("POST", f"/platform/access-requests/{obr['id']}/issue-pin")
call("POST", "/auth/login", {"email": ob_mail, "pin": "ob5678"}, expect=400)
obt = first_login(ob_mail, np_["temporary_pin"], "ob2468")
check("new one-time PIN replaces the old credentials and is replaced at first sign-in", obt["profile"]["user"]["must_change_pin"] is False)
audit_ob = call("GET", "/audit?period=today&module=platform", token=obt["token"], branch="none")["items"]
check("PIN issue audited without the PIN", any(a["action"] == "reset_pin" for a in audit_ob) and np_["temporary_pin"] not in json.dumps(audit_ob))
# Resend welcome / retry / delivery tracking
rw = call("POST", f"/platform/access-requests/{obr['id']}/resend-welcome")
check("welcome email resent with a fresh link", rw["email_status"]["status"] == "sent" and len(mails_to(ob_mail, "Welcome to S'Shop")) == 2)
ack_row = next(e for e in call("GET", f"/platform/access-requests/{obr['id']}")["emails"] if e["kind"] == "access_request_ack")
rt = call("POST", f"/platform/emails/{ack_row['id']}/retry")
check("failed or missing emails can be retried", rt["email_status"]["status"] == "sent" and len(mails_to(ob_mail, "We received")) == 2)
wid = mails_to(ob_mail, "Welcome to S'Shop")[-1]["id"]
check("unsigned delivery events refused", resend_webhook({"type": "email.delivered", "data": {"email_id": wid}}, sign=False) == 401)
check("signed delivery event accepted", resend_webhook({"type": "email.delivered", "data": {"email_id": wid}}) == 200)
hist = call("GET", f"/platform/access-requests/{obr['id']}")["emails"]
check("delivery status tracked", any(e["kind"] == "welcome" and e["status"] == "delivered" for e in hist), [(e["kind"], e["status"]) for e in hist])
check("tenant page shows the email history", any(e["kind"] == "welcome" for e in call("GET", f"/platform/tenants/{oap['tenant_id']}")["emails"]))
# Forgot PIN: same answer for everyone; a reset link for users
nobody = call("POST", "/auth/forgot", {"email": f"nobody{suffix.lower()}@sshop.test"}, token="none")
known = call("POST", "/auth/forgot", {"email": ob_mail}, token="none")
check("forgot PIN never reveals whether an email exists", nobody["message"] == known["message"])
reset = mails_to(ob_mail, "Reset your S'Shop PIN")
check("reset link emailed", len(reset) == 1 and "30 minutes" in reset[0]["text"])
time.sleep(1.1)
call("POST", "/auth/set-pin", {"token": link_token(reset[0]), "pin": "ob1111"}, token="none")
call("GET", "/auth/me", token=obt["token"], branch="none", expect=401)
check("reset completes; older sessions end", call("POST", "/auth/login", {"email": ob_mail, "pin": "ob1111"})["profile"]["user"]["must_change_pin"] is False)
for _ in range(4):
    call("POST", "/auth/forgot", {"email": ob_mail}, token="none")
time.sleep(1.5)
check("reset emails are rate-limited per address", len(mails_to(ob_mail, "Reset your S'Shop PIN")) <= 4)
call("POST", "/auth/link", {"token": "not-a-real-token"}, token="none", expect=422)
check("unknown links refused", True)
# Applicants: status only through an emailed link
pend_mail = f"pend{suffix.lower()}@sshop.test"
call("POST", "/access-requests", {**ob, "email": pend_mail, "business_name": f"Pending {suffix}"}, token="none")
mails_to(pend_mail, "We received")
call("POST", "/auth/forgot", {"email": pend_mail}, token="none")
st_mail = mails_to(pend_mail, "request status")
check("applicant gets a status link, not a reset link", len(st_mail) == 1 and not mails_to(pend_mail, "Reset"))
st_tok = link_token(st_mail[0])
st = call("POST", "/auth/request-status", {"token": st_tok}, token="none")
check("pending status with support contacts", st["status"] == "pending" and "0798 993 404" in st["support_phones"], st)
pend = next(x for x in call("GET", "/platform/access-requests?status=pending")["items"] if x["email"] == pend_mail)
call("POST", f"/platform/access-requests/{pend['id']}/approve")
st = call("POST", "/auth/request-status", {"token": st_tok}, token="none")
check("approved, set-up not done yet", st["status"] == "approved_setup", st)
before = len(mails_to(pend_mail, "Welcome to S'Shop"))
rs = call("POST", "/auth/request-status/resend-setup", {"token": st_tok}, token="none")
check("set-up instructions resent from the status page", rs["sent"] is True and len(mails_to(pend_mail, "Welcome to S'Shop")) == before + 1)
rej_mail = f"rej{suffix.lower()}@sshop.test"
call("POST", "/access-requests", {**ob, "email": rej_mail, "business_name": f"Rejected {suffix}"}, token="none")
rej = next(x for x in call("GET", "/platform/access-requests?status=pending")["items"] if x["email"] == rej_mail)
rr = call("POST", f"/platform/access-requests/{rej['id']}/reject", {"note": "internal: duplicate of another shop", "reason": "We are onboarding retail shops first."})
rmail = mails_to(rej_mail, "Your S'Shop access request")
check("courteous rejection email with the reason, never the internal note", rr["email_status"]["status"] == "sent" and rmail
      and "onboarding retail shops first" in rmail[-1]["text"] and "internal" not in rmail[-1]["text"])
call("POST", "/auth/forgot", {"email": rej_mail}, token="none")
rst = call("POST", "/auth/request-status", {"token": link_token(mails_to(rej_mail, "request status")[-1])}, token="none")
check("rejected status without internal notes", rst["status"] == "rejected" and "internal" not in json.dumps(rst), rst)

step("Roadmap 62–64: sale ownership, ownership changes, data-visibility scopes")
own_prod = call("POST", "/products", {"name": f"Owner Test {suffix}", "marked_price": 1000, "cost_price": 400})["result"]["id"]
call("POST", "/stock/receive", {"product_id": own_prod, "quantity": 60, "cost_price": 400})
call("POST", "/stock/receive", {"product_id": own_prod, "quantity": 20, "cost_price": 400}, branch=b2)
roles_ = {r["name"]: r["id"] for r in call("GET", "/roles")}


def staff_user(tag, role, branches):
    mail = f"{tag}{suffix.lower()}@sshop.test"
    u = call("POST", "/users", {"name": f"{tag.title()} {suffix}", "email": mail, "pin": "4321", "role_id": roles_[role], "all_branches": False, "branch_ids": branches})
    tok = call("POST", "/auth/login", {"email": mail, "pin": "4321"})["token"]
    return u["id"], tok


sp1, sp1_tok = staff_user("ownera", "Salesperson", [BRANCH])
sp2, sp2_tok = staff_user("ownerb", "Salesperson", [BRANCH])
sk, _sk_tok = staff_user("ownersk", "Storekeeper", [BRANCH])
SP1 = {"token": sp1_tok, "branch": BRANCH}
SP2 = {"token": sp2_tok, "branch": BRANCH}
owners_ = call("GET", f"/sales/owners?branch_id={BRANCH}")["items"]
check("eligible owners: active salespeople of the branch, not storekeepers", any(o["id"] == sp1 for o in owners_) and not any(o["id"] == sk for o in owners_))
line = {"items": [{"product_id": own_prod, "quantity": 1, "unit_price": 1000}], "payment": {"method": "cash"}}
s1 = call("POST", "/sales", {**line, "owner_id": sp1})["sale"]
check("recorded by one person, credited to another", s1["user_name"] == f"Ownera {suffix}" and s1["recorded_by_name"] and s1["recorded_by_name"] != s1["user_name"], s1)
call("POST", "/sales", {**line, "owner_id": sk}, expect=422)
check("only eligible owners can be credited", True)
call("POST", "/sales", {**line, "owner_id": sp2}, **SP1, expect=403)
check("assigning another owner needs “Assign sale owner”", True)
s_own = call("POST", "/sales", line, **SP1)["sale"]
check("default owner: the signed-in user", s_own["user_name"] == f"Ownera {suffix}")
# Own scope: a salesperson sees only sales credited to them
mine1 = call("GET", "/sales?period=today&limit=200", **SP1)
check("own scope: only own sales (incl. those recorded by someone else for them)", mine1["scope"] == "own"
      and {s1["id"], s_own["id"]} <= {x["id"] for x in mine1["items"]} and all(x["owner_id"] == sp1 for x in mine1["items"]))
call("GET", f"/sales/{s1['id']}", **SP2, expect=403)
call("GET", f"/sales?period=today&user_id={sp2}", **SP1, expect=403)
check("others' sales stay hidden, also through filters", True)
md_before = call("GET", "/dashboard?period=today&mine=true", **SP2)
biz_before = call("GET", "/dashboard?period=today")
# Ownership change → approval → recalculated attribution
call("POST", f"/sales/{s1['id']}/owner-change", {"new_owner_id": sp2, "reason": ""}, **SP1, expect=400)
oc = call("POST", f"/sales/{s1['id']}/owner-change", {"new_owner_id": sp2, "reason": "Ownerb served the customer"}, **SP1)
check("ownership change goes for approval; owner unchanged meanwhile", oc["pending_approval"] is True
      and call("GET", f"/sales/{s1['id']}")["sale"]["user_name"] == f"Ownera {suffix}", oc)
call("POST", f"/sales/{s1['id']}/owner-change", {"new_owner_id": sp2, "reason": "again please"}, **SP1, expect=422)
check("one pending change per sale", True)
call("POST", f"/approvals/{oc['approval_id']}/approve", {}, **SP1, expect=403)
check("requester cannot approve their own change", True)
call("POST", f"/approvals/{oc['approval_id']}/approve", {})
after = call("GET", f"/sales/{s1['id']}")
check("approved: new owner, same total, payments untouched, history kept", after["sale"]["user_name"] == f"Ownerb {suffix}"
      and float(after["sale"]["total"]) == float(s1["total"]) and len(after["payments"]) == 1
      and after["owner_changes"][0]["status"] == "approved" and after["owner_changes"][0]["from_owner"] == f"Ownera {suffix}", after["owner_changes"])
md_after = call("GET", "/dashboard?period=today&mine=true", **SP2)
biz_after = call("GET", "/dashboard?period=today")
check("performance moves to the new owner; business totals unchanged",
      round(float(md_after["kpis"]["sales"]) - float(md_before["kpis"]["sales"]), 2) == 1000.0
      and float(biz_after["kpis"]["sales"]) == float(biz_before["kpis"]["sales"]),
      (md_before["kpis"]["sales"], md_after["kpis"]["sales"]))
check("old owner no longer sees it; new owner does", s1["id"] not in {x["id"] for x in call("GET", "/sales?period=today&limit=200", **SP1)["items"]}
      and s1["id"] in {x["id"] for x in call("GET", "/sales?period=today&limit=200", **SP2)["items"]})
oc2 = call("POST", f"/sales/{s1['id']}/owner-change", {"new_owner_id": sp1, "reason": "Back to Ownera"}, **SP2)
call("POST", f"/approvals/{oc2['approval_id']}/reject", {"comments": "No"})
check("rejected: original owner kept", call("GET", f"/sales/{s1['id']}")["sale"]["user_name"] == f"Ownerb {suffix}"
      and call("GET", f"/sales/{s1['id']}")["owner_changes"][0]["status"] == "rejected")
aud_oc = call("GET", f"/audit?period=today&entity_id={s1['id']}&limit=20")["items"]
check("ownership change audited with reason", any(a["action"] == "owner_change" and "served the customer" in (a.get("comments") or "") for a in aud_oc), [a["action"] for a in aud_oc])
# Leaderboards and reports follow the scope
call("POST", "/roles", {"name": f"Own analyst {suffix}", "description": "", "permissions": ["dashboard.view", "sales.create", "sales.view", "scope.leaderboards.own"]})
roles_ = {r["name"]: r["id"] for r in call("GET", "/roles")}
an_id, an_tok = staff_user("ownan", f"Own analyst {suffix}", [BRANCH])
call("POST", "/sales", {**line, "owner_id": an_id})
lb = call("GET", "/leaderboards/staff?period=today", token=an_tok, branch=BRANCH)
check("own-scope leaderboard: only the user's own row", [x["id"] for x in lb["items"]] == [an_id], [x["name"] for x in lb["items"]][:3])
lb_all = call("GET", "/leaderboards/staff?period=today")
check("full leaderboard credits Sale Owners", any(x["id"] == sp2 for x in lb_all["items"]))
# Scopes on roles: assigned branches vs all branches
vw = call("POST", "/roles", {"name": f"Branch viewer {suffix}", "description": "", "permissions": ["sales.view", "scope.sales.branches"]})["id"]
call("POST", "/roles", {"name": f"Bad scope {suffix}", "description": "", "permissions": ["sales.view", "scope.sales.own", "scope.sales.all"]}, expect=400)
check("one scope per area", True)
aw = call("POST", "/roles", {"name": f"All viewer {suffix}", "description": "", "permissions": ["sales.view", "scope.sales.all"]})["id"]
roles_ = {r["name"]: r["id"] for r in call("GET", "/roles")}
b2_sale = call("POST", "/sales", line, branch=b2)["sale"]
_v1, v1_tok = staff_user("viewa", f"Branch viewer {suffix}", [BRANCH])
_v2, v2_tok = staff_user("viewb", f"All viewer {suffix}", [BRANCH])
seen_b = {x["id"] for x in call("GET", "/sales?period=today&limit=500", token=v1_tok, branch=BRANCH)["items"]}
check("assigned branches: other employees' sales at their branch, not other branches", s_own["id"] in seen_b and b2_sale["id"] not in seen_b)
call("GET", f"/sales?period=today&branch_id={b2}", token=v1_tok, branch=BRANCH, expect=403)
check("cannot ask for an unassigned branch", True)
seen_a = {x["id"] for x in call("GET", f"/sales?period=today&limit=500&branch_id={b2}", token=v2_tok, branch=BRANCH)["items"]}
check("all branches: sees other branches' sales without being assigned there", b2_sale["id"] in seen_a)
call("GET", f"/sales/{b2_sale['id']}", token=v2_tok, branch=BRANCH)
call("POST", "/sales", line, token=v2_tok, branch=BRANCH, expect=403)
check("visibility does not grant operations (cannot record sales)", True)
# User-specific exceptions
acc = call("GET", f"/users/{sp1}/access")
check("effective access shown per area", next(x for x in acc["scopes"] if x["area"] == "sales")["effective"] == "own", acc["scopes"][:1])
call("PUT", f"/users/{sp1}/access", {"overrides": ["scope.sales.branches"], "default_branch_id": BRANCH})
check("default branch saved and on the profile", call("POST", "/auth/login", {"email": f"ownera{suffix.lower()}@sshop.test", "pin": "4321"})["profile"]["user"]["default_branch_id"] == BRANCH)
check("user exception widens one area", s_own["id"] in {x["id"] for x in call("GET", "/sales?period=today&limit=500", **SP1)["items"]}
      and call("GET", "/sales?period=today&limit=500", **SP1)["scope"] == "branches")
call("PUT", f"/users/{sp1}/access", {"overrides": ["-sales.request_owner_change"]})
call("POST", f"/sales/{s_own['id']}/owner-change", {"new_owner_id": sp2, "reason": "Should be refused"}, **SP1, expect=403)
check("explicit restriction removes a role permission", True)
call("PUT", f"/users/{sp1}/access", {"overrides": ["reports.export", "-reports.export"]}, expect=400)
call("PUT", f"/users/{sp1}/access", {"overrides": ["sales.cancel"]}, expect=400)
call("PUT", f"/users/{sp1}/access", {"overrides": []})
check("contradictory or unknown exceptions refused", True)
mgr_id, mgr_tok = staff_user("ownmgr", "Manager", [BRANCH])
call("PUT", f"/users/{sp1}/access", {"overrides": ["scope.sales.all"]}, token=mgr_tok, branch=BRANCH, expect=403)
check("nobody grants a wider scope than their own", True)

step("Roadmap 65–67: digital receipts, sharing, adjustment receipts, approved exchanges")
rc_cust = "07" + str(uuid.uuid4().int)[:8]
rc_full = call("POST", "/sales", {**line, "owner_id": sp1, "customer": {"mobile": rc_cust, "first_name": "Ruth"},
                                  "items": [{"product_id": own_prod, "quantity": 3, "unit_price": 1000}], "payment": {"method": "cash"}})
rc_sale = rc_full["sale"]
rcs = call("GET", f"/sales/{rc_sale['id']}/receipts")["items"]
snap = rcs[0]["snapshot"]
check("receipt issued with the sale: number, items, total, owner, signature", len(rcs) == 1 and rcs[0]["kind"] == "original"
      and snap["number"] == rc_sale["receipt_no"] and snap["title"] == "SALES RECEIPT" and snap["items"][0]["qty"] == 3
      and float(snap["totals"]["total"]) == 3000 and snap["served_by"] == f"Ownera {suffix}" and snap["customer"] == "Ruth"
      and snap["signed_by"], snap.get("served_by"))
# Immutable: later changes never alter an issued receipt
oc3 = call("POST", f"/sales/{rc_sale['id']}/owner-change", {"new_owner_id": sp2, "reason": "Receipt immutability check"}, **SP1)
call("POST", f"/approvals/{oc3['approval_id']}/approve", {})
again = call("GET", f"/sales/{rc_sale['id']}/receipts")["items"]
check("an issued receipt never changes (owner changed later)", again[0]["snapshot"]["served_by"] == f"Ownera {suffix}"
      and call("GET", f"/sales/{rc_sale['id']}")["sale"]["user_name"] == f"Ownerb {suffix}")
# Secure link and public receipt
lnk = call("POST", f"/receipts/{rcs[0]['id']}/link")["url"]
tok = lnk.rsplit("/r/", 1)[1]
pub = call("GET", f"/r/{tok}", token="none", branch="none")
check("secure link opens the same receipt without signing in", pub["snapshot"]["number"] == rc_sale["receipt_no"] and "/r/" in lnk)
call("GET", f"/r/{uuid.uuid4()}", token="none", branch="none", expect=404)
check("unknown receipt links refused", True)
# WhatsApp: a short message with the link, not a long text receipt
sh = call("POST", f"/sales/{rc_sale['id']}/share")
check("WhatsApp message is short and links to the receipt", "/r/" in sh["text"] and "Ruth" in sh["text"] and "•" not in sh["text"] and sh["link"].startswith("https://wa.me/"), sh["text"])
# Email with the PDF attached, from the business's name
fake_pdf = base64.b64encode(b"%PDF-1.4\n% receipt\n").decode()
call("POST", f"/receipts/{rcs[0]['id']}/email", {"to": "ruth@sshop.test", "pdf": base64.b64encode(b"not a pdf").decode()}, expect=400)
em = call("POST", f"/receipts/{rcs[0]['id']}/email", {"to": "ruth@sshop.test", "pdf": fake_pdf})
rmail = mails_to("ruth@sshop.test", rc_sale["receipt_no"])
check("receipt emailed with the PDF attached, from the business", em["email_status"]["status"] == "sent" and rmail
      and rmail[-1]["attachments"][0]["filename"].endswith(".pdf") and rmail[-1]["from"].split("<")[0].strip() == snap["business"]["name"], rmail[-1].get("from") if rmail else None)
# Receipt configuration applies to receipts issued from now on
cfg = call("GET", "/settings")["settings"]
cfg["sales"]["receipt"]["show_customer"] = False
cfg["sales"]["receipt"]["font"] = "thermal"
call("PUT", "/settings", cfg)
rc2 = call("POST", "/sales", {**line, "customer": {"mobile": rc_cust, "first_name": "Ruth"}})["sale"]
s2 = call("GET", f"/sales/{rc2['id']}/receipts")["items"][0]["snapshot"]
check("receipt settings shape new receipts only", s2["customer"] is None and s2["font"] == "thermal"
      and call("GET", f"/sales/{rc_sale['id']}/receipts")["items"][0]["snapshot"]["customer"] == "Ruth")
cfg["sales"]["receipt"]["font"] = "fancy"
call("PUT", "/settings", cfg, expect=400)
cfg["sales"]["receipt"]["font"] = "sans"
cfg["sales"]["receipt"]["show_customer"] = True
call("PUT", "/settings", cfg)
# Return → adjustment receipt linked to the original; original preserved
rline = rc_full["items"][0]["id"]
call("POST", f"/sales/{rc_sale['id']}/return", {"items": [{"sale_item_id": rline, "quantity": 1}], "reason": "Customer changed mind", "restock": True})
rcs = call("GET", f"/sales/{rc_sale['id']}/receipts")["items"]
adj = next(r for r in rcs if r["kind"] == "adjustment")["snapshot"]["adjustment"]
check("adjustment receipt: reference, status, refund, net value; original unchanged", len(rcs) == 2
      and adj["original_receipt"] == rc_sale["receipt_no"] and adj["reference"].startswith("RTN-")
      and next(r for r in rcs if r["kind"] == "adjustment")["snapshot"]["status"] == "Partially Returned"
      and float(adj["refund"]) == 1000 and float(adj["net_sale_value"]) == 2000 and adj["refund_method"] == "Cash"
      and rcs[0]["snapshot"]["items"][0]["qty"] == 3, adj)
lp = adj["loyalty"]
check("loyalty reconciled on the adjustment: earned, reversed, unrecovered, net", lp["original"] == rc_sale["points_earned"]
      and lp["net"] == lp["original"] - lp["reversed"] and lp["unrecovered"] >= 0
      and (lp["original"] == 0 or lp["reversed"] > 0), lp)
# Exchange above the approval limit: nothing changes until approved
swap = call("POST", "/products", {"name": f"Swap Item {suffix}", "marked_price": 1500, "cost_price": 600})["result"]["id"]
call("POST", "/stock/receive", {"product_id": swap, "quantity": 10, "cost_price": 600})
call("PUT", "/settings/workflows/sale.return", {"enabled": True, "min_amount": None, "levels": [{"approver_type": "role", "approver_role_id": roles_["Manager"]}]})
xs2 = call("POST", "/sales", {**line, "owner_id": sp1, "items": [{"product_id": own_prod, "quantity": 2, "unit_price": 1000}]})["sale"]
xs2_line = call("GET", f"/sales/{xs2['id']}")["items"][0]["id"]
def swap_on_hand():
    return call("GET", f"/stock?q=Swap Item {suffix}")["items"][0]["on_hand"]
swap_before = swap_on_hand()
px = call("POST", f"/sales/{xs2['id']}/exchange", {"return_items": [{"sale_item_id": xs2_line, "quantity": 1}],
                                                   "items": [{"product_id": swap, "quantity": 1, "unit_price": 1500}],
                                                   "payment": {"method": "cash"}, "reason": "Different size", "client_ref": str(uuid.uuid4())})
pend_x = call("GET", f"/sales/{xs2['id']}")
check("exchange waits for approval: stock, sale and receipts untouched", px.get("pending_approval") is True
      and swap_on_hand() == swap_before
      and pend_x["items"][0]["returned_qty"] == 0 and len(call("GET", f"/sales/{xs2['id']}/receipts")["items"]) == 1)
call("POST", f"/approvals/{px['approval_id']}/approve", {}, token=mgr_tok, branch=BRANCH)
done_x = call("GET", f"/sales/{xs2['id']}")
xr = call("GET", f"/sales/{xs2['id']}/receipts")["items"]
xadj = next(r for r in xr if r["kind"] == "adjustment")["snapshot"]
check("approved exchange executed once: items back, replacement sold, exchange receipt", done_x["items"][0]["returned_qty"] == 1
      and swap_on_hand() == swap_before - 1
      and xadj["status"] == "Exchanged" and xadj["adjustment"]["exchange"] is not None and xadj["title"] == "EXCHANGE RECEIPT", xadj.get("status"))
repl = call("GET", f"/sales?period=today&q={xadj['adjustment']['exchange']['receipt_no']}")["items"]
check("replacement credited to the original owner (no new attribution)", repl and repl[0]["owner_id"] == sp1, repl[:1])
call("POST", f"/approvals/{px['approval_id']}/approve", {}, token=mgr_tok, branch=BRANCH, expect=422)
check("an approval cannot run twice", True)
call("PUT", "/settings/workflows/sale.return", {"enabled": False, "min_amount": None, "levels": [{"approver_type": "admin"}]})

step("Roadmap 69: new-order notifications and the Orders badge")
call("POST", "/stock/receive", {"product_id": own_prod, "quantity": 10, "cost_price": 400})
n_slug = call("GET", "/auth/me")["tenant"]["slug"]
n_mobile = "07" + str(uuid.uuid4().int)[:8]
n_tok = call("POST", f"/portal/{n_slug}/session", {"mobile": n_mobile, "first_name": "Wanjiru"}, token="none")["token"]
def badge(**kw):
    return call("GET", "/notifications?limit=50", **kw)
b0 = badge()["new_orders"]
call("PUT", "/auth/preferences", {"language": "en", "font": "Outfit", "currency": "KES", "notify_new_orders": False}, token=mgr_tok, branch=BRANCH)
no1 = call("POST", f"/portal/{n_slug}/orders", {"items": [{"product_id": own_prod, "quantity": 2}], "delivery_location": "Shop"}, token=n_tok)
feed = badge()
note = next((n for n in feed["items"] if n["link"] == f"/orders/{no1['id']}"), None)
check("customer order notifies order staff: customer, branch, items, total, link", note is not None
      and note["title"] == f"New customer order {no1['order_no']}" and "Customer: Wanjiru" in note["body"] and "2 items" in note["body"]
      and note["read_at"] is None, note)
check("Orders badge counts new orders", feed["new_orders"] == b0 + 1, (b0, feed["new_orders"]))
mgr_feed = badge(token=mgr_tok, branch=BRANCH)
check("alerts switched off: no notification, badge still counts", not any(n["link"] == f"/orders/{no1['id']}" for n in mgr_feed["items"])
      and mgr_feed["new_orders"] >= 1)
check("own-records orders scope: not notified about customers' orders", not any(n["link"] == f"/orders/{no1['id']}" for n in badge(**SP1)["items"]))
call("POST", "/notifications/read-all")
check("reading notifications leaves the Orders badge unchanged", badge()["new_orders"] == b0 + 1)
call("POST", f"/orders/{no1['id']}/status", {"status": "confirmed"})
check("confirming the order lowers the badge", badge()["new_orders"] == b0)
call("PUT", "/auth/preferences", {"language": "en", "font": "Outfit", "currency": "KES", "notify_new_orders": True}, token=mgr_tok, branch=BRANCH)
no2 = call("POST", f"/portal/{n_slug}/orders", {"items": [{"product_id": own_prod, "quantity": 1}], "delivery_location": "Shop"}, token=n_tok)
check("alerts back on: notified again, once", sum(1 for n in badge(token=mgr_tok, branch=BRANCH)["items"] if n["link"] == f"/orders/{no2['id']}") == 1)

step("Roadmap 70–73: tenants, secure business access, several businesses per tenant")
home_t = login["profile"]["tenant"]["id"]
tq = {"business_name": f"Tenant Co {suffix}", "contact_name": "Mwangi", "email": f"tenantco{suffix.lower()}@sshop.test", "phone": "0722 333 444",
      "location": "Nakuru", "business_type": "Retail shop", "branches": 1, "message": "Please onboard us"}
call("POST", "/access-requests", tq, token="none")
tq_id = next(x for x in call("GET", "/platform/access-requests?status=pending")["items"] if x["business_name"] == tq["business_name"])["id"]
tap = call("POST", f"/platform/access-requests/{tq_id}/approve")
ta = first_login(tq["email"], tap["temporary_pin"], "tc2468")
TA = {"token": ta["token"], "branch": "none"}
tb1 = tap["tenant_id"]
accs = call("GET", "/platform/accounts")["items"]
tacc = next(a for a in accs if tq["business_name"] in a["business_names"])
check("approved request: one tenant, its first business, primary administrator", tacc["businesses"] == 1 and tacc["admin_email"] == tq["email"]
      and tacc["status"] in ("active", "trial") and sum(1 for a in accs if tq["business_name"] in a["business_names"]) == 1, tacc)
call("GET", "/platform/accounts", **TA, expect=403)
check("tenant administrators never reach Platform → Tenants", True)
# 71 — secure business access
call("POST", f"/platform/tenants/{tb1}/support", {"pin": "wrong-pin", "reason": "Checking stock levels", "scope": "view", "minutes": 30}, expect=400)
call("POST", f"/platform/tenants/{tb1}/support", {"pin": PIN, "reason": "short", "scope": "view", "minutes": 30}, expect=400)
call("POST", f"/platform/tenants/{tb1}/support", {"pin": PIN, "reason": "Checking stock levels", "scope": "view", "minutes": 999}, expect=400)
check("support access needs a fresh PIN, a reason and a time limit", True)
sv = call("POST", f"/platform/tenants/{tb1}/support", {"pin": PIN, "reason": "Checking stock levels for the owner", "scope": "view", "minutes": 30})
SV = {"token": sv["token"], "branch": "none"}
check("view-only session: banner details on the profile", sv["status"] == "active" and sv["profile"]["acting"]["support"]["scope"] == "view"
      and sv["profile"]["tenant"]["id"] == tb1 and sv["profile"]["businesses"] == [], sv["profile"].get("acting"))
call("GET", "/products?status=all", **SV)
r = call("POST", "/categories", {"name": "Support write"}, **SV, expect=422)
check("view-only: nothing can be changed", r["error"]["title"] == "View-only support access", r)
check("administrators are told when support enters", any(n["kind"] == "support_started" for n in call("GET", "/notifications?limit=20", **TA)["items"]))
sa = call("GET", "/support-access", **TA)
check("the business sees the session and its reason", sa["policy"] == "notify" and sa["items"][0]["status"] == "active"
      and sa["items"][0]["reason"].startswith("Checking"), sa["items"][:1])
call("POST", f"/support-access/{sv['id']}/revoke", **TA)
r = call("GET", "/auth/me", **SV, expect=422)
check("revoked by the business: the session stops at once", r["error"]["title"] == "Support session ended", r)
call("PUT", "/support-access/policy", {"policy": "approval"}, **TA)
rq = call("POST", f"/platform/tenants/{tb1}/support", {"pin": PIN, "reason": "Fix a pricing problem for the owner", "scope": "full", "minutes": 60})
check("approval policy: the request waits for the business", rq["status"] == "requested" and "token" not in rq, rq)
call("POST", f"/platform/support/{rq['id']}/start", {"pin": PIN}, expect=422)
check("cannot start before approval", True)
call("POST", f"/support-access/{rq['id']}/approve", **TA)
st = call("POST", f"/platform/support/{rq['id']}/start", {"pin": PIN})
SF = {"token": st["token"], "branch": "none"}
call("POST", "/categories", {"name": f"Support cat {suffix}"}, **SF)
check("approved full session works and is audited in the business", any(a["action"] == "support_started" for a in call("GET", "/audit?period=today&limit=200", **TA)["items"]))
call("PUT", "/support-access/policy", {"policy": "notify"}, **SF, expect=403)
cfg_t = call("GET", "/settings", **SF)["settings"]
cfg_t["security"]["support_access"] = "notify"
call("PUT", "/settings", cfg_t, **SF)
check("support can never change the business's consent policy", call("GET", "/support-access", **TA)["policy"] == "approval")
call("POST", f"/support-access/{rq['id']}/approve", **SF, expect=403)
call("POST", "/auth/switch-business", {"tenant_id": home_t}, **SF, expect=422)
check("no approving or switching from inside a support session", True)
endr = call("POST", f"/platform/support/{st['id']}/end", **SF)
check("ending returns the platform owner to their own business", endr["profile"]["acting"] is None and endr["profile"]["tenant"]["id"] == home_t)
r = call("GET", "/auth/me", **SF, expect=401)
check("the ended support token is closed (sign in again, not an inactivity message)", r["error"]["code"] == "unauthorized", r)
call("PUT", "/support-access/policy", {"policy": "notify"}, **TA)
# 72 — several businesses per tenant, one sign-in
nb = call("POST", f"/platform/accounts/{tacc['id']}/businesses", {"name": f"Tenant Co Two {suffix}"})
tb2 = nb["tenant_id"]
me_ta = call("GET", "/auth/me", **TA)
check("second business: the primary administrator can switch to it", sorted(b["id"] for b in me_ta["businesses"]) == sorted([tb1, tb2]), me_ta["businesses"])
# Switching from a second sign-in (another device): TA stays signed in to the first business.
ta_dev2 = call("POST", "/auth/login", {"email": tq["email"], "pin": "tc2468"})
sw = call("POST", "/auth/switch-business", {"tenant_id": tb2}, token=ta_dev2["token"], branch="none")
TB2 = {"token": sw["token"], "branch": "none"}
r = call("GET", "/auth/me", token=ta_dev2["token"], branch="none", expect=401)
check("switching replaces that sign-in's session (the old token is closed)", r["error"]["code"] == "unauthorized", r)
_exp = lambda tok: json.loads(base64.urlsafe_b64decode(tok.split(".")[1] + "==="))["exp"]
check("switching never extends a session's end", _exp(sw["token"]) <= _exp(ta_dev2["token"]), (_exp(sw["token"]), _exp(ta_dev2["token"])))
check("switched without signing in again; separate data", sw["profile"]["tenant"]["id"] == tb2 and "*" in sw["profile"]["permissions"]
      and call("GET", "/products?status=all", **TB2)["items"] == [] and not any(c["name"] == f"Support cat {suffix}" for c in call("GET", "/categories", **TB2)))
call("POST", "/auth/switch-business", {"tenant_id": home_t}, **TB2, expect=403)
check("never into another tenant", True)
roles_b2 = {r["name"]: r["id"] for r in call("GET", "/roles", **TB2)}
roles_b1 = {r["name"]: r["id"] for r in call("GET", "/roles", **TA)}
b2_branch = sw["profile"]["branches"][0]["id"]
ca_mail = f"clerka{suffix.lower()}@sshop.test"
ca = call("POST", "/users", {"name": "Clerk A", "email": ca_mail, "pin": "5678", "role_id": roles_b1["Salesperson"], "all_branches": True, "branch_ids": []}, **TA)
lk = call("GET", "/users/linkable", **TB2)["items"]
check("people of the tenant's other businesses can be added (never the platform owner)", any(x["id"] == ca["id"] for x in lk) and not any(x["email"] == EMAIL for x in lk))
call("POST", "/users/link", {"user_id": login["profile"]["user"]["id"], "role_id": roles_b2["Salesperson"], "all_branches": True}, **TB2, expect=400)
call("POST", "/users/link", {"user_id": ca["id"], "role_id": roles_b2["Salesperson"], "all_branches": False, "branch_ids": [b2_branch]}, **TB2)
ca_login = call("POST", "/auth/login", {"email": ca_mail, "pin": "5678"})
check("one sign-in lists both businesses", len(ca_login["profile"]["businesses"]) == 2)
ca2 = call("POST", "/auth/switch-business", {"tenant_id": tb2}, token=ca_login["token"], branch="none")
check("the other business's role and branches apply", ca2["profile"]["user"]["role"] == "Salesperson" and [b["id"] for b in ca2["profile"]["branches"]] == [b2_branch]
      and ca2["profile"]["linked_from"] == tq["business_name"], ca2["profile"]["user"])
check("listed as linked in that business", any(u.get("linked_from") == tq["business_name"] for u in call("GET", "/users", **TB2)))
call("POST", "/auth/change-pin", {"current_pin": "5678", "new_pin": "8765"}, token=ca2["token"], branch=b2_branch)
check("one PIN for every business", bool(call("POST", "/auth/login", {"email": ca_mail, "pin": "8765"})["token"]))
call("PUT", f"/users/{ca['id']}", {"name": "Clerk A", "email": ca_mail, "role_id": roles_b1["Salesperson"], "all_branches": True, "branch_ids": [], "is_active": False}, **TA)
call("GET", "/auth/me", token=ca2["token"], branch=b2_branch, expect=401)
check("deactivating the person ends their access in every business", True)
# 70 — tenant-wide status
call("POST", f"/platform/accounts/{tacc['id']}/status", {"status": "deactivated", "reason": "Contract ended"})
tdet = call("GET", f"/platform/accounts/{tacc['id']}")
check("deactivating a tenant deactivates every business", tdet["account"]["status"] == "deactivated" and all(b["status"] == "deactivated" for b in tdet["businesses"]))
call("GET", "/auth/me", **TA, expect=401)
call("POST", f"/platform/accounts/{tacc['id']}/status", {"status": "active", "reason": "Renewed"})
home_acc = call("GET", "/platform/accounts")["home_account_id"]
call("POST", f"/platform/accounts/{home_acc}/status", {"status": "deactivated", "reason": "Should be refused"}, expect=422)
check("platform-owned tenant protected", True)
# 73 — tenant activity
act_t = call("GET", f"/platform/activity?account_id={tacc['id']}&activity=support&period=today")
check("tenant activity: support sessions, only this tenant", act_t["totals"]["support"] >= 4
      and all(x["business"] in (tq["business_name"], f"Tenant Co Two {suffix}") for x in act_t["items"]), act_t["totals"])

step("Roadmap 83: Quick Login PIN on trusted devices")
qp_roles = {r["name"]: r["id"] for r in call("GET", "/roles")}
qp_mail = f"quick{suffix.lower()}@sshop.test"
qp_user = call("POST", "/users", {"name": f"Quick {suffix}", "email": qp_mail, "pin": "4321", "role_id": qp_roles["Manager"], "all_branches": True, "branch_ids": []})["id"]
QF = {"token": call("POST", "/auth/login", {"email": qp_mail, "pin": "4321"})["token"], "branch": BRANCH}
st = call("GET", "/auth/quick-pin", **QF)
check("available, not set yet, 4–6 digits", st["available"] and not st["enabled"] and st["min_length"] == 4 and st["devices"] == [], st)
call("PUT", "/auth/quick-pin", {"current_pin": "wrong", "quick_pin": "2580"}, **QF, expect=400)
call("PUT", "/auth/quick-pin", {"current_pin": "4321", "quick_pin": "1234"}, **QF, expect=400)
call("PUT", "/auth/quick-pin", {"current_pin": "4321", "quick_pin": "25801"[:3]}, **QF, expect=400)
check("needs the full PIN; weak, short or non-digit Quick PINs refused", True)
dev = call("PUT", "/auth/quick-pin", {"current_pin": "4321", "quick_pin": "2580", "device_name": "Till phone"}, **QF)
check("Quick PIN saved; this device trusted with a secret shown once", len(dev["device_token"]) >= 40 and not has_price_value(dev, 2580) and not mentions_word(dev, 2580))
r = call("POST", "/auth/quick-login", {"device_token": dev["device_token"], "pin": "2581"}, token="none", branch="none", expect=400)
check("wrong Quick PIN refused", r["error"]["message"] == "Wrong Quick PIN", r)
ql = call("POST", "/auth/quick-login", {"device_token": dev["device_token"], "pin": "2580"}, token="none", branch="none")
QQ = {"token": ql["token"], "branch": BRANCH}
check("Quick sign-in on the trusted device; the session is marked", ql["profile"]["quick"] is True and call("GET", "/auth/me", **QQ)["quick"] is True
      and call("GET", "/auth/me", **QF)["quick"] is False)
call("GET", "/sales?period=today", **QQ)
for m_, p_, b_ in [("POST", "/auth/change-pin", {"current_pin": "4321", "new_pin": "5678"}), ("PUT", "/auth/quick-pin", {"current_pin": "4321", "quick_pin": "3690"}),
                   ("POST", "/roles", {"name": "Sneaky", "permissions": ["sales.view"]}), ("PUT", "/security/quick-pin", {"enabled": False, "role_ids": []})]:
    r = call(m_, p_, b_, **QQ, expect=422)
    check(f"Quick session cannot {p_}", r["error"]["title"] == "Full sign-in needed", r)
r = call("POST", "/auth/quick-login", {"device_token": "not-a-device", "pin": "2580"}, token="none", branch="none", expect=422)
check("an untrusted device always needs the full sign-in", r["error"]["title"] == "Full sign-in needed")
audit_q = call("GET", "/audit?period=today&limit=300&module=auth")["items"]
check("Quick PIN events audited, never the PIN", any(a["action"] == "quick_pin_enabled" for a in audit_q) and not mentions_word(audit_q, 2580))
# Administrators reset (never see) it; a full PIN reset ends trusted devices too
call("POST", f"/users/{qp_user}/quick-pin/reset", {}, **QQ, expect=422)
call("POST", f"/users/{qp_user}/quick-pin/reset", {})
call("POST", "/auth/quick-login", {"device_token": dev["device_token"], "pin": "2580"}, token="none", branch="none", expect=422)
check("administrator reset: the device needs the full sign-in", True)
QF = {"token": call("POST", "/auth/login", {"email": qp_mail, "pin": "4321"})["token"], "branch": BRANCH}
dev2 = call("PUT", "/auth/quick-pin", {"current_pin": "4321", "quick_pin": "2580"}, **QF)
time.sleep(1.1)  # sessions are compared by the second
call("POST", f"/users/{qp_user}/reset-pin", {"pin": "8642"})
call("POST", "/auth/quick-login", {"device_token": dev2["device_token"], "pin": "2580"}, token="none", branch="none", expect=422)
call("GET", "/auth/me", **QF, expect=401)
check("full PIN reset ends trusted devices and older sessions", True)
# Lock after repeated wrong PINs
QF = {"token": call("POST", "/auth/login", {"email": qp_mail, "pin": "8642"})["token"], "branch": BRANCH}
dev3 = call("PUT", "/auth/quick-pin", {"current_pin": "8642", "quick_pin": "3691"}, **QF)
for _ in range(5):
    call("POST", "/auth/quick-login", {"device_token": dev3["device_token"], "pin": "0000"}, token="none", branch="none", expect=400)
call("POST", "/auth/quick-login", {"device_token": dev3["device_token"], "pin": "3691"}, token="none", branch="none", expect=403)
check("five wrong Quick PINs lock it (even the right one) for a while", True)
# Business rule: off revokes every trusted device of the business
call("PUT", "/security/quick-pin", {"enabled": False, "role_ids": []})
r = call("PUT", "/auth/quick-pin", {"current_pin": "8642", "quick_pin": "4826"}, **QF, expect=422)
check("business switched Quick PIN off: cannot be set", r["error"]["title"] == "Quick PIN unavailable", r)
check("…and trusted devices were revoked", call("GET", "/auth/quick-pin", **QF)["devices"] == [])
call("PUT", "/security/quick-pin", {"enabled": True, "role_ids": [str(uuid.uuid4())]}, expect=400)
call("PUT", "/security/quick-pin", {"enabled": True, "role_ids": []})
call("PUT", "/security/quick-pin", {"enabled": True, "role_ids": []}, **QF, expect=403)
check("only administrators set the business rule", True)
# Platform rules
ps = call("GET", "/platform/security")
check("platform rules readable by the platform owner", ps["quick_pin_enabled"] and ps["quick_pin_min_length"] == 4)
call("PUT", "/platform/security", {**ps, "quick_pin_min_length": 7}, expect=400)
call("GET", "/platform/security", **QF, expect=403)
call("PUT", "/platform/security", {**ps, "quick_pin_min_length": 5})
r = call("PUT", "/auth/quick-pin", {"current_pin": "8642", "quick_pin": "4826"}, **QF, expect=400)
check("platform minimum length applies", "5–6" in r["error"]["message"], r)
call("PUT", "/platform/security", ps)

step("Roadmap 87: automatic logout after inactivity")
sp_roles = {r["name"]: r["id"] for r in call("GET", "/roles")}
pol = call("GET", "/security/sessions")
check("session policy with the platform's limits", pol["platform"]["min_minutes"] == 1 and pol["mine"]["warning_seconds"] >= 10, pol)
call("PUT", "/security/sessions", {**pol["policy"], "idle_minutes": 0}, expect=400)
call("PUT", "/security/sessions", {**pol["policy"], "idle_minutes": pol["platform"]["max_minutes"] + 1}, expect=400)
call("PUT", "/security/sessions", {**pol["policy"], "role_minutes": [{"role_id": sp_roles["Salesperson"], "minutes": pol["policy"]["idle_minutes"] + 5}]}, expect=400)
call("PUT", "/security/sessions", {**pol["policy"], "warning_seconds": 5}, expect=400)
check("timeouts within S'Shop's limits; role timeouts only stricter", True)
call("PUT", "/security/sessions", {**pol["policy"], "role_minutes": [{"role_id": sp_roles["Salesperson"], "minutes": 1}]})
it_mail = f"idle{suffix.lower()}@sshop.test"
call("POST", "/users", {"name": f"Idle {suffix}", "email": it_mail, "pin": "4321", "role_id": sp_roles["Salesperson"], "all_branches": True, "branch_ids": []})
il = call("POST", "/auth/login", {"email": it_mail, "pin": "4321"})
IT = {"token": il["token"], "branch": BRANCH}
check("a stricter role timeout applies (1 minute, 30-second warning)", il["profile"]["session"] == {"idle_seconds": 60, "warning_seconds": 30}, il["profile"]["session"])
check("the heartbeat extends the session and returns the limits", call("POST", "/auth/activity", {}, **IT)["idle_seconds"] == 60)
time.sleep(95)
r = call("GET", "/auth/me", **IT, activity=False, expect=401)
check("idle past the limit: the server ends the session", r["error"]["code"] == "session_expired" and "inactivity" in r["error"]["message"], r)
call("POST", "/auth/activity", {}, **IT, expect=401)
check("…and it cannot be revived by a late heartbeat", True)
check("expiry audited", any(a["action"] == "session_expired" for a in call("GET", "/audit?period=today&limit=300&module=auth")["items"]))
il2 = call("POST", "/auth/login", {"email": it_mail, "pin": "4321"})
IT2 = {"token": il2["token"], "branch": BRANCH}
time.sleep(40)
call("GET", "/notifications?limit=5", **IT2, activity=False)
call("GET", "/auth/me", **IT2, activity=False)
time.sleep(58)
r = call("GET", "/auth/me", **IT2, activity=False, expect=401)
check("background requests do not keep a session alive", r["error"]["code"] == "session_expired")
il3 = call("POST", "/auth/login", {"email": it_mail, "pin": "4321"})
IT3 = {"token": il3["token"], "branch": BRANCH}
call("POST", "/auth/logout", {}, **IT3)
r = call("GET", "/auth/me", **IT3, expect=401)
check("log out ends the session on the server (not reported as inactivity)", r["error"]["code"] == "unauthorized", r)
call("PUT", "/security/sessions", pol["policy"])

step("Roadmap 91: M-Pesa per business — accounts, verification, STK Push sales")
def mpesa_callback(cid, code, desc, amount=None, receipt=None):
    """Answers a prompt as Safaricom would: POST to the callback URL the server gave with the request."""
    meta = [] if code != 0 else [{"Name": "Amount", "Value": amount}, {"Name": "MpesaReceiptNumber", "Value": receipt},
                                 {"Name": "TransactionDate", "Value": 20261011101010}, {"Name": "PhoneNumber", "Value": 254712345678}]
    cb = {"Body": {"stkCallback": {"MerchantRequestID": "m-" + cid, "CheckoutRequestID": cid, "ResultCode": code, "ResultDesc": desc,
                                   **({"CallbackMetadata": {"Item": meta}} if meta else {})}}}
    url = STK[cid]["req"]["CallBackURL"]
    req = urllib.request.Request(BASE + url[url.index("/api/"):], data=json.dumps(cb).encode(), method="POST", headers={"Content-Type": "application/json"})
    return urllib.request.urlopen(req).status

if PS_PORT:
    pm = call("GET", "/mpesa/platform")
    check("platform M-Pesa (Railway variables): status without secrets", pm["configured"] is False and "public_url" in pm, pm)
    r = call("POST", "/mpesa/platform/test", {}, expect=422)
    check("platform test refused clearly when not configured", r["error"]["title"] == "Platform M-Pesa not in use", r)
    call("GET", "/mpesa/platform", **T2, expect=403)
    check("only the platform owner sees the platform's M-Pesa", True)
    acc0 = call("GET", "/mpesa/accounts")
    check("no account yet: STK Push not available", acc0["items"] == [] and not acc0["stk"]["available"], acc0["stk"])
    base_acc = {"name": "Main Till", "kind": "till", "shortcode": "174379", "till_number": "5123456", "environment": "sandbox",
                "consumer_key": DARAJA["key"], "consumer_secret": DARAJA["secret"], "passkey": "wrong-passkey", "all_branches": True}
    call("POST", "/mpesa/accounts", {**base_acc, "shortcode": "12ab"}, expect=400)
    call("POST", "/mpesa/accounts", {**base_acc, "kind": "till", "till_number": ""}, expect=400)
    call("POST", "/mpesa/accounts", {**base_acc, "passkey": ""}, expect=400)
    check("account details validated (digits, till number, credentials)", True)
    acc_id = call("POST", "/mpesa/accounts", base_acc)["id"]
    raw = json.dumps(call("GET", "/mpesa/accounts"), ensure_ascii=False)
    check("secrets never leave the server (key shown as its last 4 only)", DARAJA["secret"] not in raw and DARAJA["key"] not in raw
          and "wrong-passkey" not in raw and "••••1234" in raw)
    tc = call("POST", f"/mpesa/accounts/{acc_id}/test", {})
    check("test connection: Safaricom accepts the key and secret", tc["ok"], tc)
    tp = call("POST", f"/mpesa/accounts/{acc_id}/test-prompt", {"phone": "0712345678"})
    check("wrong passkey: refused, explained, and the error kept", not tp["ok"] and "Password" in tp["message"] and "passkey does not match" in tp["message"]
          and "Password" in (call("GET", "/mpesa/accounts")["items"][0]["last_error"] or ""), tp)
    r = call("PUT", f"/mpesa/accounts/{acc_id}/stk", {"enabled": True}, expect=422)
    check("STK Push stays off until verified", r["error"]["title"] == "Verify the account first", r)
    upd = call("PUT", f"/mpesa/accounts/{acc_id}", {**base_acc, "consumer_key": "", "consumer_secret": "", "passkey": DARAJA["passkey"]})
    check("a new passkey needs verification again", upd["needs_verification"])
    tp = call("POST", f"/mpesa/accounts/{acc_id}/test-prompt", {"phone": "254712345678"})
    check("test prompt accepted: account verified", tp["ok"] and call("GET", "/mpesa/accounts")["items"][0]["verified_at"], tp)
    call("PUT", f"/mpesa/accounts/{acc_id}/stk", {"enabled": True})
    me_now = call("GET", "/auth/me")
    check("STK Push on: the till offers it", me_now["integrations"]["mpesa_stk"] and me_now["integrations"]["mpesa_stk_branches"] is None)
    till_req = [v["req"] for v in STK.values()][-1]
    check("sandbox: Safaricom's test shortcode as a Paybill, never the business's own till; callback per account",
          till_req["TransactionType"] == "CustomerPayBillOnline" and till_req["BusinessShortCode"] == "174379" and till_req["PartyB"] == "174379"
          and f"/mpesa/a/{acc_id}/" in till_req["CallBackURL"], till_req)
    accs = call("GET", "/mpesa/accounts")["items"]
    check("checks shown apart: OAuth, request accepted, test payment", accs[0]["oauth_ok_at"] and accs[0]["verified_at"] and accs[0]["last_test_status"] == "pending", accs[0])
    # Production: the business's own Store number + Till number, as Buy Goods (a separate account, never auto-enabled).
    prod_acc = call("POST", "/mpesa/accounts", {**base_acc, "name": "Live Till", "shortcode": "7928587", "till_number": "4489262",
                                                 "environment": "production", "passkey": DARAJA["passkey"]})["id"]
    call("POST", f"/mpesa/accounts/{prod_acc}/test-prompt", {"phone": "0712345678"})
    live_req = [v["req"] for v in STK.values()][-1]
    check("production Buy Goods: Store number as shortcode, Till number as PartyB", live_req["TransactionType"] == "CustomerBuyGoodsOnline"
          and live_req["BusinessShortCode"] == "7928587" and live_req["PartyB"] == "4489262", live_req)
    call("POST", f"/mpesa/accounts/{prod_acc}/test-prompt", {"phone": "0712345678", "amount": 500}, expect=400)
    check("a test payment is limited to KSh 1–100", True)
    cs = call("POST", f"/mpesa/accounts/{prod_acc}/check-status", {})
    check("check payment status: still waiting while Safaricom is processing", cs["status"] == "pending" and not cs["ok"], cs)
    STK[list(STK)[-1]]["query"] = (0, "The service request is processed successfully.")
    cs = call("POST", f"/mpesa/accounts/{prod_acc}/check-status", {})
    check("check payment status: confirmed by Safaricom on demand", cs["ok"] and cs["status"] == "success", cs)
    check("…and shown as a confirmed test payment", next(x for x in call("GET", "/mpesa/accounts")["items"] if x["id"] == prod_acc)["last_test_status"] == "success")
    call("POST", "/mpesa/platform/check-status", {}, expect=422)
    # A forged callback (wrong token) changes nothing.
    bad_cb = urllib.request.Request(BASE + f"/api/webhooks/mpesa/a/{acc_id}/not-the-token", data=b"{}", method="POST", headers={"Content-Type": "application/json"})
    try:
        urllib.request.urlopen(bad_cb)
        forged = 200
    except urllib.error.HTTPError as e:
        forged = e.code
    check("callbacks need the account's secret token", forged == 404, forged)

    # A product with 3 in stock.
    stk_p = call("POST", "/products", {"name": f"STK Kettle {suffix}", "marked_price": 1500, "cost_price": 900})["result"]["id"]
    call("POST", "/stock/receive", {"product_id": stk_p, "quantity": 3, "cost_price": 900})
    lvl = lambda: call("GET", f"/stock?q=STK Kettle {suffix}")["items"][0]
    def stk_sale(qty, ref=None, phone="0712345678", expect=200):
        return call("POST", "/sales/stk", {"phone": phone, "sale": {"items": [{"product_id": stk_p, "quantity": qty, "unit_price": 1500}],
                                                                    "payment": {"method": "mpesa"}, "client_ref": ref or str(uuid.uuid4())}}, expect=expect)
    ref1 = str(uuid.uuid4())
    ps = stk_sale(2, ref1)
    check("STK sale waits for payment: pending, amount from the sale", ps["status"] == "awaiting" and float(ps["amount"]) == 3000
          and ps["request"]["status"] == "pending" and ps["sale"] is None, ps)
    l1 = lvl()
    check("its stock is held (2 reserved, 1 available)", l1["reserved"] == 2 and l1["available"] == 1 and l1["on_hand"] == 3, l1)
    stk_sale(2, expect=422)
    check("another cashier cannot sell the held units", True)
    pushes = len(STK)
    again = stk_sale(2, ref1)
    check("a second tap returns the waiting sale, no second prompt", again["id"] == ps["id"] and len(STK) == pushes)
    cid1 = [k for k, v in STK.items() if v["req"]["Amount"] == 3000][-1]
    check("never paid on initiation", call("GET", f"/sales/stk/{ps['id']}")["status"] == "awaiting")
    mpesa_callback(cid1, 0, "The service request is processed successfully.", 3000, "SSK1A2B3C4")
    done = call("GET", f"/sales/stk/{ps['id']}")
    sale1 = (done.get("sale") or {}).get("sale") or {}
    check("confirmed by Safaricom: the sale completes once, with the M-Pesa receipt", done["status"] == "completed" and sale1.get("receipt_no")
          and any(p["reference"] == "SSK1A2B3C4" for p in (done.get("sale") or {}).get("payments", [])), done)
    l2 = lvl()
    check("stock sold once: on hand 1, nothing reserved", l2["on_hand"] == 1 and l2["reserved"] == 0, l2)
    mpesa_callback(cid1, 0, "duplicate", 3000, "SSK1A2B3C4")
    same = [x for x in call("GET", "/sales?period=today&limit=200")["items"] if x["receipt_no"] == sale1.get("receipt_no")]
    check("a repeated callback sells nothing twice", lvl()["on_hand"] == 1 and len(same) == 1 and len(call("GET", "/sales?period=today&limit=200&payment_method=mpesa")["items"]) >= 1, len(same))

    call("POST", "/stock/receive", {"product_id": stk_p, "quantity": 4, "cost_price": 900})
    ps2 = stk_sale(1)
    cid2 = list(STK)[-1]
    mpesa_callback(cid2, 1032, "Request cancelled by user")
    c2 = call("GET", f"/sales/stk/{ps2['id']}")
    check("customer cancels: kept as cancelled, stock released", c2["status"] == "cancelled" and lvl()["reserved"] == 0, c2)

    ps3 = stk_sale(1)
    cid3 = list(STK)[-1]
    STK[cid3]["query"] = (0, "The service request is processed successfully.")
    time.sleep(21)
    c3 = call("GET", f"/sales/stk/{ps3['id']}")
    check("lost callback: the status query confirms it and the sale completes", c3["status"] == "completed", c3)
    mpesa_callback(cid3, 0, "late", 1500, "SSK9Z8Y7X6")
    s3 = call("GET", f"/sales/{c3['sale']['sale']['id']}")
    check("the late callback adds the M-Pesa receipt to the payment", any(p["reference"] == "SSK9Z8Y7X6" for p in s3["payments"]), s3["payments"])

    ps4 = stk_sale(1)
    cid4 = list(STK)[-1]
    mpesa_callback(cid4, 0, "ok", 100, "SSK5555555")
    c4 = call("GET", f"/sales/stk/{ps4['id']}")
    check("less paid than due: no sale, flagged for attention", c4["status"] == "attention" and "due" in (c4["error"] or ""), c4)
    call("POST", f"/sales/stk/{ps4['id']}/complete", {}, expect=422)
    check("…and it cannot be forced through", True)

    ps5 = stk_sale(1)
    cid5 = list(STK)[-1]
    call("POST", f"/sales/stk/{ps5['id']}/cancel", {})
    check("cancelled at the till: stock released", lvl()["reserved"] == 0)
    mpesa_callback(cid5, 0, "ok", 1500, "SSK6666666")
    c5 = call("GET", f"/sales/stk/{ps5['id']}")
    check("paid after cancelling: flagged for attention, never lost", c5["status"] == "attention", c5)
    c5b = call("POST", f"/sales/stk/{ps5['id']}/complete", {})
    check("an authorised person completes it from the record", c5b["status"] == "completed" and c5b["sale"], c5b)

    act = call("GET", "/mpesa/activity")
    check("activity: requests with outcome, phones masked", any(i["purpose"] == "test" for i in act["items"]) and all("12345678" not in (i["phone"] or "") for i in act["items"])
          and act["attention"] >= 1, act["items"][:2])
    test_req = next(i["id"] for i in act["items"] if i["purpose"] == "test")
    r = call("POST", "/sales", {"items": [{"product_id": stk_p, "quantity": 1, "unit_price": 1500}], "payment": {"method": "mpesa", "mpesa_request_id": test_req}}, expect=422)
    check("a test prompt can never pay for a sale", True)
    sp_role = next(r_["id"] for r_ in call("GET", "/roles") if r_["name"] == "Salesperson")
    sp_mail = f"mpesasp{suffix.lower()}@sshop.test"
    call("POST", "/users", {"name": "M-Pesa Clerk", "email": sp_mail, "pin": "4321", "role_id": sp_role, "all_branches": True, "branch_ids": []})
    sp_tok = call("POST", "/auth/login", {"email": sp_mail, "pin": "4321"})["token"]
    call("POST", "/mpesa/accounts", base_acc, token=sp_tok, expect=403)
    call("GET", "/mpesa/accounts", token=sp_tok, expect=403)
    check("only those allowed configure or see M-Pesa accounts", True)
    call("GET", f"/sales/stk/{ps['id']}", **T2, expect=404)
    check("another business never sees these sales or accounts", call("GET", "/mpesa/accounts", **T2)["items"] == [])
    d = call("DELETE", f"/mpesa/accounts/{acc_id}")
    check("an account with payments is switched off, not deleted", d["deactivated"] and not call("GET", "/mpesa/accounts")["items"][0]["is_active"])
    check("…and STK Push stops", not call("GET", "/auth/me")["integrations"]["mpesa_stk"])

step("Roadmap 98: barcodes — entry, uniqueness, generation, stocktaking by barcode")
call("POST", "/products", {"name": f"Bad Code {suffix}", "marked_price": 100, "barcode": "12 34"}, expect=400)
check("barcodes are checked for shape (3–64 letters, digits, - . _ / +)", True)
z = call("POST", "/products", {"name": f"Zero Lead {suffix}", "marked_price": 120, "barcode": f"00{suffix}9876"})["result"]["id"]
check("leading zeros kept (stored as text)", call("GET", f"/products/{z}")["product"]["barcode"] == f"00{suffix}9876")
r = call("POST", "/products", {"name": f"Clash {suffix}", "marked_price": 120, "barcode": f"00{suffix}9876"}, expect=422)
check("a barcode already used names the product that has it", f"Zero Lead {suffix}" in r["error"]["message"], r)
g1, g2 = call("POST", "/products/generate-barcode", {})["barcode"], call("POST", "/products/generate-barcode", {})["barcode"]
def ean_ok(c):
    s = sum(int(d) * (1 if i % 2 == 0 else 3) for i, d in enumerate(c[:12]))
    return len(c) == 13 and c.isdigit() and c[0] == "2" and (10 - s % 10) % 10 == int(c[12])
check("generated barcodes: in-store EAN-13 with a valid check digit, unique", ean_ok(g1) and ean_ok(g2) and g1 != g2, (g1, g2))
check("no products share a barcode", call("GET", "/products/barcode-conflicts")["items"] == [])
# Stock take, unit by unit: 3 units received, 2 found → the third is recorded as lost (an adjustment, approvals apply).
tp = call("POST", "/products", {"name": f"Tracked Lamp {suffix}", "marked_price": 900, "track_items": True})["result"]["id"]
units = [f"LMP{suffix}{i}" for i in range(3)]
call("POST", "/stock/receive", {"product_id": tp, "quantity": 3, "barcodes": units})
r = call("POST", "/stock/count", {"reason": "Shelf count", "lines": [{"product_id": tp, "barcodes": [units[0], units[0]]}]}, expect=422)
check("a unit counted twice is refused", r["error"]["title"] == "Scanned twice", r)
r = call("POST", "/stock/count", {"reason": "Shelf count", "lines": [{"product_id": tp, "barcodes": [units[0], "NOT-HERE-1"]}]}, expect=422)
check("a unit that is not in stock here is refused, named", r["error"]["title"] == "Check these items" and "NOT-HERE-1" in r["error"]["message"], r)
r = call("POST", "/stock/count", {"reason": "Shelf count", "lines": [{"product_id": tp, "counted": 0}]}, expect=422)
check("a tracked product is counted by its units, never by a number", r["error"]["title"] == "Scan each item", r)
c = call("POST", "/stock/count", {"reason": "Shelf count", "lines": [{"product_id": tp, "barcodes": units[:2]}]})
lvl = call("GET", f"/stock?q=Tracked Lamp {suffix}")["items"][0]
check("the unit not found is recorded as lost (2 left), the others untouched", c["applied"] + c["pending_approval"] == 1 and (lvl["on_hand"] == 2 or c["pending_approval"] == 1), (c, lvl))
# Counting barcoded products by barcode, when the business requires it.
st = call("GET", "/settings")["settings"]
call("PUT", "/settings", {**st, "stock": {**st["stock"], "require_barcode_stocktake": True}})
r = call("POST", "/stock/count", {"reason": "Shelf count", "lines": [{"product_id": z, "counted": 0}]}, expect=422)
check("barcodes required at stock takes: a barcoded product typed in is refused", r["error"]["title"] == "Scan to count", r)
call("POST", "/stock/count", {"reason": "Shelf count", "lines": [{"product_id": z, "counted": 0, "scanned": True}]})
check("…and accepted once counted by its barcode", True)
call("PUT", "/settings", st)

step("Roadmap 99: onboarding — Excel templates, validation, approval and posting")
ov = call("GET", "/onboarding")
check("onboarding centre: its datasets, cutover, totals", [d["key"] for d in ov["datasets"]] == ["products", "customers", "opening_stock", "expenses", "sales", "credit", "loyalty", "orders", "transfers"] and "totals" in ov, [d["key"] for d in ov["datasets"]])
req = urllib.request.Request(BASE + "/api/onboarding/products/template", headers={"Authorization": "Bearer " + TOKEN})
tpl = urllib.request.urlopen(req).read()
check("templates download as Excel workbooks", tpl[:2] == b"PK" and len(tpl) > 3000, len(tpl))
main_code = next(b["code"] for b in call("GET", "/branches") if b["id"] == BRANCH)
ob = suffix.lower()
prod_rows = [
    [f"Imported Mug {suffix}", "", "Kitchen", 450, 200, f"77{suffix}001", "YES"],      # auto-numbered
    [f"Imported Jar {suffix}", f"OBJ{suffix}", "Kitchen", 600, 250, f"77{suffix}002", "YES"],
    [f"Duplicate Jar {suffix}", f"OBJ{suffix}", "Kitchen", 600, 250, f"77{suffix}003", "YES"],  # code twice in the file
    [f"Bad Price {suffix}", "", "Kitchen", "abc", 1, f"77{suffix}004", "YES"],              # not an amount
    [f"No Barcode {suffix}", "", "Kitchen", 300, 100, "", "YES"],                           # barcode required
]
prod_book = make_xlsx(["Product Name", "SKU", "Category", "Price", "Cost", "Barcode", "Barcode Required During Sale"], prod_rows)
up = upload_xlsx("products", prod_book, name=f"products-{ob}.xlsx")
b = up["batch"]
check("columns mapped by their usual names (SKU, Price, Cost, Barcode)", b["mapping"].get("SKU") == "code" and b["mapping"].get("Price") == "selling_price", b["mapping"])
check("preview: 2 valid, 3 invalid (duplicate code, bad amount, missing barcode)", b["total_rows"] == 5 and b["invalid_rows"] == 3, (b["valid_rows"], b["warning_rows"], b["invalid_rows"]))
msgs = " | ".join(m["message"] for r in up["rows"] for m in r["messages"])
check("errors explained per row", "already on row" in msgs and "not an amount" in msgs and "Barcode Number is required" in msgs, msgs[:300])
r = call("POST", f"/onboarding/batches/{b['id']}/post", {}, expect=422)
check("not posted while rows are invalid, unless explicitly asked", r["error"]["title"] == "Fix the invalid rows", r)
er = urllib.request.urlopen(urllib.request.Request(BASE + f"/api/onboarding/batches/{b['id']}/errors", headers={"Authorization": "Bearer " + TOKEN})).read()
check("error report downloads (the rows to fix)", er[:2] == b"PK")
done = import_now(b['id'], {"skip_invalid": True})
check("products: valid rows posted, invalid ones left out on request", done["status"] == "posted" and done["summary"]["created"] == 2, done)
mug = [p for p in call("GET", f"/products?q=Imported Mug {suffix}&status=all")["items"]]
check("S'Shop numbered the product without a code; barcode kept", mug and mug[0]["code"].startswith("PRD") and mug[0].get("barcode", f"77{suffix}001") == f"77{suffix}001", mug[:1])
r = upload_xlsx("products", prod_book, name=f"products-{ob}.xlsx", expect=422)
check("the same workbook cannot be imported twice", r["error"]["title"] == "Already imported", r)
upd = upload_xlsx("products", make_xlsx(["Product Code", "Product Name", "Category", "Selling Price"], [[f"OBJ{suffix}", f"Imported Jar {suffix}", "Kitchen", 650]]), mode="update")
import_now(upd['batch']['id'], {})
check("update mode changes an existing product by code", float(call("GET", f"/products?q=OBJ{suffix}&status=all")["items"][0]["marked_price"]) == 650)

known = "0744" + str(int(time.time()))[-6:]
call("POST", "/customers", {"mobile": known, "first_name": "Known"})
cust = upload_xlsx("customers", make_xlsx(["Phone Number", "First Name", "Email"], [["0733" + str(int(time.time()))[-6:], "Imported", "imp@example.com"], [known, "Known", ""], ["12345", "Bad", ""]]))
check("customers: an existing mobile is matched, an invalid one refused", cust["batch"]["warning_rows"] == 1 and cust["batch"]["invalid_rows"] == 1, cust["batch"])
cdone = import_now(cust['batch']['id'], {"skip_invalid": True})
check("customers posted: 1 created, 1 matched (not duplicated)", cdone["summary"]["created"] == 1 and cdone["summary"]["matched"] == 1, cdone)

today_s = datetime.date.today().isoformat()
osb = upload_xlsx("opening_stock", make_xlsx(["Product Code", "Branch Code", "Opening Quantity", "Unit Cost", "As-of Date"],
                                             [[f"OBJ{suffix}", main_code, 12, 250, today_s], ["NO-SUCH-CODE", main_code, 3, 1, today_s]]))
r = call("POST", f"/onboarding/batches/{osb['batch']['id']}/post", {"skip_invalid": True}, expect=422)
check("opening stock never posts with invalid rows (stock must be complete)", "stock and money" in r["error"]["message"], r)
call("POST", f"/onboarding/batches/{osb['batch']['id']}/cancel", {})
osb = upload_xlsx("opening_stock", make_xlsx(["Product Code", "Branch Code", "Opening Quantity", "Unit Cost", "As-of Date"], [[f"OBJ{suffix}", main_code, 12, 250, today_s]]))
odone = import_now(osb['batch']['id'], {})
jar = call("GET", f"/stock?q=Imported Jar {suffix}")["items"][0]
check("opening stock posted through the stock ledger (12 on hand)", odone["summary"]["quantity"] == 12 and jar["on_hand"] == 12, (odone, jar["on_hand"]))
again = upload_xlsx("opening_stock", make_xlsx(["Product Code", "Branch Code", "Opening Quantity", "As-of Date"], [[f"OBJ{suffix}", main_code, 5, today_s]]))
check("opening stock is never added on top of recorded stock", again["batch"]["invalid_rows"] == 1 and "already has stock" in again["rows"][0]["messages"][0]["message"], again["rows"][0]["messages"])

exp = upload_xlsx("expenses", make_xlsx(["Voucher No", "Date", "Branch", "Category", "Amount", "Payment Method", "Paid To"],
                                        [[f"V{suffix}1", "14/09/2026", main_code, f"Imported rent {suffix}", 45000, "Cash", "Landlord"]]))
check("expenses: day-first dates and a new category understood", exp["batch"]["invalid_rows"] == 0 and exp["rows"][0]["data"]["date"] == "2026-09-14", exp["rows"][0])
# Approval: a second administrator approves imports when the workflow is on.
call("PUT", "/settings/workflows/data.import", {"enabled": True, "levels": [{"approver_type": "admin"}], "min_amount": None})
pend = import_now(exp['batch']['id'], {})
check("with the Data import workflow on, the batch waits for approval", pend["status"] == "pending_approval", pend)
call("POST", f"/approvals/{pend['approval_id']}/approve", {"comments": "self"}, expect=403)
admin_role = next(r_["id"] for r_ in call("GET", "/roles") if r_["name"] == "Tenant Administrator")
a2 = f"importapprover{ob}@sshop.test"
call("POST", "/users", {"name": "Import Approver", "email": a2, "pin": "4321", "role_id": admin_role, "all_branches": True, "branch_ids": []})
a2t = call("POST", "/auth/login", {"email": a2, "pin": "4321"})["token"]
call("POST", f"/approvals/{pend['approval_id']}/approve", {"comments": "Checked against the vouchers"}, token=a2t)
eb = call("GET", f"/onboarding/batches/{exp['batch']['id']}")["batch"]
check("approved by someone else: posted, with who approved", eb["status"] == "posted" and eb["decided_by"] == "Import Approver", eb)
call("PUT", "/settings/workflows/data.import", {"enabled": False, "levels": [{"approver_type": "admin"}], "min_amount": None})
dup = upload_xlsx("expenses", make_xlsx(["Expense Reference", "Expense Date", "Branch Code", "Category", "Amount"], [[f"V{suffix}1", today_s, main_code, "Rent", 10]]))
check("an expense reference imported before is refused", dup["batch"]["invalid_rows"] == 1, dup["rows"][0]["messages"])
call("POST", f"/onboarding/batches/{dup['batch']['id']}/cancel", {})
tot = call("GET", "/onboarding")["totals"]
check("onboarding totals: products, customers, opening stock and expenses", tot["products"] >= 2 and tot["customers"] >= 1 and tot["opening_quantity"] >= 12 and float(tot["expenses_amount"]) >= 45000, tot)
# ── Roadmap 100: sales history, credit balances, loyalty opening points ──
call("PUT", "/onboarding/cutover", {"date": today_s})
known_id = next(c["id"] for c in call("GET", f"/customers?q={known}")["items"])
jar_before = call("GET", f"/stock?q=Imported Jar {suffix}")["items"][0]["on_hand"]
sh = [["H" + suffix + "1", "2026-03-04", main_code, f"OBJ{suffix}", 2, 600, 100, "Cash", "", known, "", ""],
      ["H" + suffix + "1", "2026-03-04", main_code, f"OBJ{suffix}", 1, 650, 0, "Cash", "", known, "", ""],
      ["H" + suffix + "2", "2026-03-05", main_code, f"OBJ{suffix}", 1, 650, 0, "M-Pesa", "QXX1", "", "", ""],
      ["H" + suffix + "3", today_s, main_code, f"OBJ{suffix}", 1, 650, 0, "Cash", "", "", "", ""],              # on the cutover
      ["H" + suffix + "4", "2026-03-06", main_code, f"OBJ{suffix}", 1, 650, 0, "Credit", "", known, "", ""],    # credit belongs elsewhere
      ["H" + suffix + "5", "2026-03-07", main_code, f"OBJ{suffix}", 1, 650, 0, "Cash", "", "", "", ""],
      ["H" + suffix + "5", "2026-03-08", main_code, f"OBJ{suffix}", 1, 650, 0, "Cash", "", "", "", ""]]         # same receipt, other date
heads = ["Receipt Reference", "Sale Date", "Branch Code", "Product Code", "Quantity", "Unit Selling Price", "Discount", "Payment Method", "Payment Reference", "Customer Mobile", "Salesperson Email", "Notes"]
hs = upload_xlsx("sales", make_xlsx(heads, sh))
bad = {r_["row_no"]: " ".join(m["message"] for m in r_["messages"]) for r_ in hs["rows"] if r_["status"] == "invalid"}
check("sales history: after the cutover, credit and mismatched receipt lines refused", "cutover" in bad.get(5, "") and "Credit sales template" in bad.get(6, "") and "must match its first line" in bad.get(8, ""), bad)
call("POST", f"/onboarding/batches/{hs['batch']['id']}/post", {"skip_invalid": True}, expect=422)
check("sales history never imports with invalid rows (money must be complete)", True)
call("POST", f"/onboarding/batches/{hs['batch']['id']}/cancel", {})
hs = upload_xlsx("sales", make_xlsx(heads, sh[:3]))
check("valid history: 3 lines, 2 receipts", hs["batch"]["invalid_rows"] == 0 and hs["batch"]["total_rows"] == 3, hs["batch"])
hd = import_now(hs['batch']['id'], {})
check("posted as 2 historical sales worth 1,950 + 650", hd["summary"]["created"] == 2 and float(hd["summary"]["amount"]) == 2500, hd)
h1 = next(x for x in call("GET", "/sales?period=all&limit=200")["items"] if x["receipt_no"] == "H" + suffix + "1")
h1d = call("GET", f"/sales/{h1['id']}")
check("original receipt number and date kept; marked historical", h1d["sale"]["business_date"] == "2026-03-04" and h1d["sale"]["is_legacy"] and float(h1d["sale"]["total"]) == 1850, h1d["sale"])
check("its lines and payment recorded (discount kept)", len(h1d["items"]) == 2 and float(h1d["sale"]["discount_total"]) == 100 and any(p_["method"] == "cash" and float(p_["amount"]) == 1850 for p_ in h1d["payments"]), h1d["payments"])
check("no stock deducted and no points awarded", call("GET", f"/stock?q=Imported Jar {suffix}")["items"][0]["on_hand"] == jar_before and h1d["sale"]["points_earned"] == 0)
kc = call("GET", f"/customers/{known_id}")
kcust = kc.get("customer", kc)
check("customer totals include the history; last purchase is the sale date, not today", float(kcust["total_spend"]) >= 1850 and str(kcust["last_purchase_at"])[:10] == "2026-03-04", (kcust["total_spend"], kcust["last_purchase_at"]))
again = upload_xlsx("sales", make_xlsx(heads, [sh[2]]))
check("a receipt already imported is refused", again["batch"]["invalid_rows"] == 1 and "already a receipt" in again["rows"][0]["messages"][0]["message"])
call("POST", f"/onboarding/batches/{again['batch']['id']}/cancel", {})

ch = ["Credit Reference", "Customer Mobile", "Branch Code", "Original Sale Date", "Due Date", "Original Amount", "Amount Paid", "Product Code", "Quantity"]
cr = upload_xlsx("credit", make_xlsx(ch, [["CR" + suffix + "1", known, main_code, "2026-02-01", "2026-03-01", 12000, 5000, f"OBJ{suffix}", 2],
                                          ["CR" + suffix + "2", known, main_code, "2026-01-10", "2026-02-10", 3000, 0, "", ""],
                                          ["CR" + suffix + "3", known, main_code, "2026-02-01", "2026-01-01", 100, 0, "", ""],      # due before sale
                                          ["CR" + suffix + "4", known, main_code, "2026-02-01", "2026-03-01", 100, 150, "", ""]]))  # paid > original
cbad = {r_["row_no"]: " ".join(m["message"] for m in r_["messages"]) for r_ in cr["rows"]}
check("credit: due date before the sale and over-payment refused; no product = opening balance", "before the Original Sale Date" in cbad[4] and "between 0 and the Original Amount" in cbad[5] and "Opening Credit Balance" in cbad[3], cbad)
call("POST", f"/onboarding/batches/{cr['batch']['id']}/cancel", {})
cr = upload_xlsx("credit", make_xlsx(ch, [["CR" + suffix + "1", known, main_code, "2026-02-01", "2026-03-01", 12000, 5000, f"OBJ{suffix}", 2],
                                          ["CR" + suffix + "2", known, main_code, "2026-01-10", "2026-02-10", 3000, 0, "", ""]]))
cd_ = import_now(cr['batch']['id'], {})
c1 = call("GET", f"/credit?q=CR{suffix}1")["items"][0]
c2 = call("GET", f"/credit?q=CR{suffix}2")["items"][0]
check("credit imported with balances and original due dates (both past due: shown overdue, aging from the due date)", float(c1["balance"]) == 7000 and float(c1["amount_paid"]) == 5000
      and c1["due_date"] == "2026-03-01" and float(c2["balance"]) == 3000 and c1["status"] == "overdue" and c2["status"] == "overdue", (c1, c2))
check("credit never moves stock", call("GET", f"/stock?q=Imported Jar {suffix}")["items"][0]["on_hand"] == jar_before)
check("onboarding totals: historical sales and outstanding credit", float(call("GET", "/onboarding")["totals"]["credit_outstanding"]) >= 10000)

lh = ["Customer Mobile", "Points Balance", "Points Earned", "Points Redeemed", "Effective Date"]
ly = upload_xlsx("loyalty", make_xlsx(lh, [[known, 300, 500, 100, today_s]]))
check("loyalty: earned − redeemed must equal the balance", ly["batch"]["invalid_rows"] == 1 and "must equal" in ly["rows"][0]["messages"][0]["message"], ly["rows"][0]["messages"])
call("POST", f"/onboarding/batches/{ly['batch']['id']}/cancel", {})
pts_before = int((kc.get("customer", kc))["points_available"])
ly = upload_xlsx("loyalty", make_xlsx(lh, [[known, 400, 500, 100, today_s]]))
import_now(ly['batch']['id'], {})
kc2 = call("GET", f"/customers/{known_id}")
check("opening points added once (+400 available, history kept as earned and redeemed)", int((kc2.get("customer", kc2))["points_available"]) == pts_before + 400)
ly2 = upload_xlsx("loyalty", make_xlsx(lh, [[known, 50, "", "", today_s]]))
check("opening points are never allocated twice", ly2["batch"]["invalid_rows"] == 1 and "already imported" in ly2["rows"][0]["messages"][0]["message"])
call("POST", f"/onboarding/batches/{ly2['batch']['id']}/cancel", {})
# ── Roadmap 101: orders, transfers, reconciliation, completion ──
wl_code = f"WL{suffix}"
oh = ["Order Reference", "Order Date", "Customer Mobile", "Branch Code", "Product Code", "Quantity", "Unit Price", "Order Status", "Delivery Location", "Notes"]
orows = [["O" + suffix + "1", "2026-03-10", known, main_code, f"OBJ{suffix}", 2, 650, "Completed", "Kilimani", ""],     # history
         ["O" + suffix + "2", "2026-03-20", known, main_code, f"OBJ{suffix}", 2, 650, "Confirmed", "Kilimani", ""],     # open, holds stock
         ["O" + suffix + "3", "2026-03-21", known, main_code, f"OBJ{suffix}", 1, 650, "New", "", ""],                   # open, nothing held
         ["O" + suffix + "4", "2026-03-21", known, main_code, f"OBJ{suffix}", 1, 650, "Lost", "", ""],                  # unknown status
         ["O" + suffix + "5", "2026-03-22", known, main_code, f"OBJ{suffix}", 50, 650, "Preparing", "", ""],            # more than the stock
         ["O" + suffix + "6", today_s, known, main_code, f"OBJ{suffix}", 1, 650, "Delivered", "", ""],                  # closed on the cutover
         ["O" + suffix + "7", "2026-03-23", known, main_code, f"OBJ{suffix}", 1, 650, "New", "", ""],
         ["O" + suffix + "7", "2026-03-24", known, main_code, f"OBJ{suffix}", 1, 650, "New", "", ""]]                  # same order, other date
ob_o = upload_xlsx("orders", make_xlsx(oh, orows))
obad = {r_["row_no"]: " ".join(m["message"] for m in r_["messages"]) for r_ in ob_o["rows"] if r_["status"] == "invalid"}
check("orders: unknown status, stock not available, closed on the cutover and mismatched lines refused",
      "Order Status" in obad.get(5, "") and "only" in obad.get(6, "") and "cutover" in obad.get(7, "") and "same date" in obad.get(9, "") and len(obad) == 4, obad)
call("POST", f"/onboarding/batches/{ob_o['batch']['id']}/post", {"skip_invalid": True}, expect=422)
call("POST", f"/onboarding/batches/{ob_o['batch']['id']}/cancel", {})
jar_main = call("GET", f"/stock?q=Imported Jar {suffix}&branch_id={BRANCH}")["items"][0]
ob_o = upload_xlsx("orders", make_xlsx(oh, orows[:3]))
od = import_now(ob_o['batch']['id'], {})
check("orders posted: 3 orders, 2 units held for the confirmed one", od["summary"]["created"] == 3 and od["summary"]["quantity"] == 2, od["summary"])
olist = call("GET", f"/orders?q=O{suffix}&status=all&limit=50")["items"]
o1 = call("GET", f"/orders/{next(x['id'] for x in olist if x['order_no'] == 'O' + suffix + '1')}")
o2 = call("GET", f"/orders/{next(x['id'] for x in olist if x['order_no'] == 'O' + suffix + '2')}")
check("closed order is history: no sale, nothing held, never moves again", o1["order"]["is_legacy"] and o1["order"]["sale_id"] is None and not o1["order"]["reserved"] and o1["next_statuses"] == [], o1["order"])
call("POST", f"/orders/{o1['order']['id']}/status", {"status": "cancelled"}, expect=422)
check("open order keeps its date and holds its stock; it carries on in S'Shop", str(o2["order"]["created_at"])[:10] == "2026-03-20" and o2["order"]["reserved"] and "preparing" in o2["next_statuses"], o2["order"])
jar_after = call("GET", f"/stock?q=Imported Jar {suffix}&branch_id={BRANCH}")["items"][0]
check("the open order's units are reserved, on hand unchanged", jar_after["on_hand"] == jar_main["on_hand"] and jar_after.get("reserved", 0) == jar_main.get("reserved", 0) + 2, (jar_main, jar_after))
again = upload_xlsx("orders", make_xlsx(oh, [orows[0]]))
check("an order already in S'Shop is refused", again["batch"]["invalid_rows"] == 1 and "already in S'Shop" in again["rows"][0]["messages"][0]["message"])
call("POST", f"/onboarding/batches/{again['batch']['id']}/cancel", {})

th = ["Transfer Reference", "Origin Branch Code", "Destination Branch Code", "Product Code", "Quantity Dispatched", "Quantity Received", "Transfer Date", "Receipt Date", "Transfer Status", "Notes"]
trows = [["T" + suffix + "1", main_code, wl_code, f"OBJ{suffix}", 5, 4, "2026-03-01", "2026-03-02", "Received", ""],
         ["T" + suffix + "2", main_code, wl_code, f"OBJ{suffix}", 3, "", "2026-03-30", "", "In transit", ""],
         ["T" + suffix + "3", main_code, wl_code, f"OBJ{suffix}", 1, "", "2026-03-30", "", "Pending", ""],
         ["T" + suffix + "4", main_code, main_code, f"OBJ{suffix}", 1, "", "2026-03-30", "", "Pending", ""],      # same branch
         ["T" + suffix + "5", main_code, wl_code, f"OBJ{suffix}", 2, 2, "2026-03-05", "2026-03-04", "Received", ""],  # received before sent
         ["T" + suffix + "6", main_code, wl_code, f"OBJ{suffix}", 2, 2, "2026-03-05", "", "In transit", ""]]      # in transit yet received
tb = upload_xlsx("transfers", make_xlsx(th, trows))
tbad = {r_["row_no"]: " ".join(m["message"] for m in r_["messages"]) for r_ in tb["rows"] if r_["status"] == "invalid"}
check("transfers: same branch, receipt before dispatch and received-while-in-transit refused",
      "must differ" in tbad.get(5, "") and "before the Transfer Date" in tbad.get(6, "") and "leave Quantity Received empty" in tbad.get(7, "") and len(tbad) == 3, tbad)
call("POST", f"/onboarding/batches/{tb['batch']['id']}/cancel", {})
wl_jar = lambda: next((x["on_hand"] for x in call("GET", f"/stock?q=Imported Jar {suffix}&branch_id={b2}", branch=b2)["items"]), 0)
wl_before = wl_jar()
tb = upload_xlsx("transfers", make_xlsx(th, trows[:3]))
td = import_now(tb['batch']['id'], {})
check("transfers posted: 3 transfers, 9 units", td["summary"]["created"] == 3 and td["summary"]["quantity"] == 9, td["summary"])
tl = call("GET", "/transfers?limit=200")["items"]
tmap = {x["transfer_no"]: x for x in tl if x["transfer_no"].startswith("T" + suffix)}
check("received as history, in transit as dispatched, pending as a draft", tmap["T" + suffix + "1"]["status"] == "received" and tmap["T" + suffix + "2"]["status"] == "dispatched" and tmap["T" + suffix + "3"]["status"] == "draft", {k: v["status"] for k, v in tmap.items()})
t1 = call("GET", f"/transfers/{tmap['T' + suffix + '1']['id']}")
check("received history keeps the short quantity", any(i_["short"] == 1 for i_ in t1["items"]) if "short" in t1["items"][0] else any(i_.get("short_qty") == 1 for i_ in t1["items"]), t1["items"])
check("importing transfers never moves stock", call("GET", f"/stock?q=Imported Jar {suffix}&branch_id={BRANCH}")["items"][0]["on_hand"] == jar_main["on_hand"] and wl_jar() == wl_before)
call("POST", f"/transfers/{tmap['T' + suffix + '2']['id']}/receive", branch=b2)
check("receiving the in-transit transfer in S'Shop books it in at the destination only",
      wl_jar() == wl_before + 3 and call("GET", f"/stock?q=Imported Jar {suffix}&branch_id={BRANCH}")["items"][0]["on_hand"] == jar_main["on_hand"])

rec = call("GET", "/onboarding/reconciliation")
rl = {(l_["dataset"]): l_ for l_ in rec["lines"] if l_["batch_id"] in (ob_o["batch"]["id"], tb["batch"]["id"])}
check("reconciliation: orders 3,250 and transfers 9 units, no difference", float(rl["orders"]["source"]) == 3250 and float(rl["orders"]["difference"]) == 0
      and float(rl["transfers"]["source"]) == 9 and float(rl["transfers"]["difference"]) == 0, rl)
check("every imported batch reconciles", rec["differences"] == 0, [l_ for l_ in rec["lines"] if float(l_["difference"]) != 0])
only = call("GET", "/onboarding/reconciliation?dataset=orders")
check("reconciliation filters by dataset", only["lines"] and all(l_["dataset"] == "orders" for l_ in only["lines"]))
req = urllib.request.Request(BASE + "/api/onboarding/reconciliation/export", headers={"Authorization": "Bearer " + TOKEN, "X-Branch-Id": BRANCH})
check("reconciliation report downloads as Excel", urllib.request.urlopen(req).read()[:2] == b"PK")
waiting = upload_xlsx("orders", make_xlsx(oh, [["O" + suffix + "9", "2026-03-21", known, main_code, f"OBJ{suffix}", 1, 650, "New", "", ""]]))
check("an unfinished import is listed", any(u["id"] == waiting["batch"]["id"] for u in call("GET", "/onboarding/reconciliation")["unfinished"]))
call("POST", "/onboarding/complete", {}, expect=422)
check("onboarding cannot be completed while an import is unfinished", True)
call("POST", f"/onboarding/batches/{waiting['batch']['id']}/cancel", {})
leftover = [u for u in call("GET", "/onboarding/reconciliation")["unfinished"]]
for u in leftover:
    call("POST", f"/onboarding/batches/{u['id']}/cancel", {})
call("POST", "/onboarding/complete", {})
check("reconciled: onboarding completed", call("GET", "/onboarding")["completed_at"] is not None)
# ── Roadmap 103: imports run in the background with real progress, a summary and a report ──
check("import progress: completed, every row counted, with an import reference", done["progress"]["stage"] == "completed" and done["progress"]["done"] == done["progress"]["total"] == 2
      and str(done["progress"]["reference"]).startswith("IMP"), done["progress"])
check("import summary: processed, imported, created, updated, left out, failed", {k: done["summary"][k] for k in ("total", "posted", "created", "updated", "skipped", "failed")}
      == {"total": 5, "posted": 2, "created": 2, "updated": 0, "skipped": 3, "failed": 0}, done["summary"])
call("POST", f"/onboarding/batches/{b['id']}/post", {"skip_invalid": True}, expect=422)
check("an import cannot be submitted again", True)
rep = urllib.request.urlopen(urllib.request.Request(BASE + f"/api/onboarding/batches/{b['id']}/report", headers={"Authorization": "Bearer " + TOKEN})).read()
check("import report downloads (summary and every row)", rep[:2] == b"PK" and b"xl/worksheets/sheet2.xml" in rep)
mug_code = mug[0]["code"]
osh = ["Product Code", "Branch Code", "Opening Quantity", "As-of Date"]
first = upload_xlsx("opening_stock", make_xlsx(osh, [[mug_code, main_code, 5, today_s]]), name=f"mug-a-{ob}.xlsx")
second = upload_xlsx("opening_stock", make_xlsx(osh, [[mug_code, main_code, 7, today_s]]), name=f"mug-b-{ob}.xlsx")
check("two opening stock files for the same product, both valid on upload", first["batch"]["invalid_rows"] == 0 and second["batch"]["invalid_rows"] == 0)
import_now(first["batch"]["id"])
fail = import_now(second["batch"]["id"])
check("data changed before posting: the import fails clearly, it never hangs", fail["status"] == "failed" and "no longer valid" in (fail["progress"]["error"] or "") and fail["progress"]["stage"] == "failed", fail["progress"])
mug_stock = call("GET", f"/stock?q=Imported Mug {suffix}&branch_id={BRANCH}")["items"][0]["on_hand"]
check("a failed import saves nothing (the first file's 5 only)", mug_stock == 5, mug_stock)
again = call("POST", f"/onboarding/batches/{second['batch']['id']}/revalidate", {})
check("a failed import can be checked again: the row shows why", again["batch"]["status"] == "validated" and again["batch"]["invalid_rows"] == 1, again["batch"])
call("POST", f"/onboarding/batches/{second['batch']['id']}/cancel", {})
spr = next(r_["id"] for r_ in call("GET", "/roles") if r_["name"] == "Salesperson")
s_mail = f"importclerk{ob}@sshop.test"
call("POST", "/users", {"name": "Import Clerk", "email": s_mail, "pin": "4321", "role_id": spr, "all_branches": True, "branch_ids": []})
call("GET", "/onboarding", token=call("POST", "/auth/login", {"email": s_mail, "pin": "4321"})["token"], expect=403)
check("only people allowed to onboard data see the centre", True)

step("Roadmap 106: loyalty earned on the payable amount; redemption window, minimum and the 50% cap")
cfg106 = call("GET", "/settings")["settings"]
orig106 = json.loads(json.dumps(cfg106))
cfg106["loyalty"].update({"enabled": True, "threshold": 1000, "points_per": 1, "min_spend": 0, "point_value": 1, "redemption_enabled": True,
                          "min_redemption_points": 10, "redemption_window_open": True, "redemption_opens_at": None, "redemption_closes_at": None,
                          "max_redemption_percent": 50, "earn_base": "net", "earn_on_redeemed": False, "rounding": "down", "redemption_needs_approval": False})
call("PUT", "/settings", cfg106)
for bad_, why in [({"max_redemption_percent": 60}, "above 50%"), ({"earn_base": "list"}, "unknown base"),
                  ({"redemption_opens_at": "2026-12-02T00:00:00Z", "redemption_closes_at": "2026-12-01T00:00:00Z"}, "closes before it opens")]:
    call("PUT", "/settings", {**cfg106, "loyalty": {**cfg106["loyalty"], **bad_}}, expect=400)
    check(f"loyalty settings refused: {why}", True)
lp106 = call("POST", "/products", {"name": f"Loyalty Sofa {suffix}", "marked_price": 100000, "max_discount": 10000, "cost_price": 60000})["result"]["id"]
call("POST", "/stock/receive", {"product_id": lp106, "quantity": 10, "cost_price": 60000})
m106 = "07" + str(int(time.time() * 7))[-8:]
sale106 = lambda extra=None, price=100000, token=None: call("POST", "/sales", {"customer": {"mobile": m106, "first_name": "Wanjiru"},
    "items": [{"product_id": lp106, "quantity": 1, "unit_price": price}], "payment": {"method": "cash"}, "client_ref": str(uuid.uuid4()), **(extra or {})}, token=token)
s1 = sale106(price=95000)["sale"]
check("brief example: 100,000 − 5,000 discount = 95,000 payable → 95 points", s1["points_earned"] == 95 and float(s1["discount_total"]) == 5000, (s1["points_earned"], s1["discount_total"]))
call("PUT", "/settings", {**cfg106, "loyalty": {**cfg106["loyalty"], "earn_base": "gross"}})
s2 = sale106(price=95000)["sale"]
check("gross base (when chosen): 100 points before the discount", s2["points_earned"] == 100, s2["points_earned"])
call("PUT", "/settings", {**cfg106, "loyalty": {**cfg106["loyalty"], "point_value": 1000}})
c106 = s1["customer"]["id"]
r = call("POST", "/sales", {"customer_id": c106, "items": [{"product_id": lp106, "quantity": 1, "unit_price": 100000}],
                            "payment": {"method": "cash"}, "redeem_points": 51, "client_ref": str(uuid.uuid4())}, expect=422)
check("points can never pay more than 50% (51 × 1,000 on 100,000)", "at most 50%" in r["error"]["message"], r)
s3 = call("POST", "/sales", {"customer_id": c106, "items": [{"product_id": lp106, "quantity": 1, "unit_price": 100000}],
                             "payment": {"method": "cash"}, "redeem_points": 50, "client_ref": str(uuid.uuid4())})["sale"]
check("exactly 50% redeemed: 50,000 by points, 50,000 by cash", float(s3["redeemed_value"]) == 50000 and float(s3["total"]) == 50000, (s3["redeemed_value"], s3["total"]))
check("no points earned on the part paid with points (50 on the 50,000 paid)", s3["points_earned"] == 50, s3["points_earned"])
call("PUT", "/settings", {**cfg106, "loyalty": {**cfg106["loyalty"], "point_value": 1000, "redemption_window_open": False}})
redeem106 = lambda pts=5, expect=200: call("POST", "/sales", {"customer_id": c106, "items": [{"product_id": lp106, "quantity": 1, "unit_price": 100000}],
                                                               "payment": {"method": "cash"}, "redeem_points": pts, "client_ref": str(uuid.uuid4())}, expect=expect)
r = redeem106(expect=422)
check("window closed: points cannot be redeemed", "window is closed" in r["error"]["message"], r)
r = call("POST", f"/customers/{c106}/redeem", {"points": 5, "notes": "gift"}, expect=422)
check("window closed: no redemption outside a sale either", "window is closed" in r["error"]["message"], r)
cv106 = call("GET", f"/customers/{c106}")
check("window closed: points still visible", cv106.get("customer", cv106)["points_available"] > 0, cv106.get("customer", cv106)["points_available"])
future = (datetime.datetime.now(datetime.timezone.utc) + datetime.timedelta(days=2)).strftime("%Y-%m-%dT%H:%M:%SZ")
call("PUT", "/settings", {**cfg106, "loyalty": {**cfg106["loyalty"], "point_value": 1000, "redemption_opens_at": future}})
r = redeem106(expect=422)
check("scheduled window not open yet: refused", "not opened yet" in r["error"]["message"], r)
call("PUT", "/settings", {**cfg106, "loyalty": {**cfg106["loyalty"], "point_value": 1000, "min_redemption_points": 100000}})
r = redeem106(expect=422)
check("below the minimum balance: refused", "minimum balance" in r["error"]["message"], r)
call("PUT", "/settings", {**cfg106, "loyalty": {**cfg106["loyalty"], "point_value": 1000, "redemption_needs_approval": True}})
sp106 = next(r_["id"] for r_ in call("GET", "/roles") if r_["name"] == "Salesperson")
sp_mail = f"cashier106{suffix.lower()}@sshop.test"
call("POST", "/users", {"name": "Cashier 106", "email": sp_mail, "pin": "4321", "role_id": sp106, "all_branches": True, "branch_ids": []})
SP106 = call("POST", "/auth/login", {"email": sp_mail, "pin": "4321"})["token"]
body106 = {"customer_id": c106, "items": [{"product_id": lp106, "quantity": 1, "unit_price": 100000}], "payment": {"method": "cash"}, "redeem_points": 5}
r = call("POST", "/sales", {**body106, "client_ref": str(uuid.uuid4())}, token=SP106, expect=422)
check("approval required: a cashier's redemption asks for a supervisor", "supervisor" in r["error"]["message"], r)
ok106 = call("POST", "/sales", {**body106, "client_ref": str(uuid.uuid4()), "supervisor": {"email": EMAIL, "pin": PIN}}, token=SP106)["sale"]
check("approved by a supervisor: redeemed", int(ok106["redeemed_points"]) == 5, ok106["redeemed_points"])
call("PUT", "/settings", orig106)

step("Roadmap 107: split payments (cash + M-Pesa + points) and partial payment left on credit")
cfg107 = call("GET", "/settings")["settings"]
orig107 = json.loads(json.dumps(cfg107))
cfg107["loyalty"].update({"enabled": True, "threshold": 1000, "points_per": 1, "min_spend": 0, "point_value": 1000, "redemption_enabled": True,
                          "min_redemption_points": 1, "redemption_window_open": True, "redemption_opens_at": None, "redemption_closes_at": None,
                          "max_redemption_percent": 50, "earn_base": "net", "earn_on_redeemed": False, "rounding": "down", "redemption_needs_approval": False})
cfg107["sales"]["mpesa_manual_confirmation"] = True
call("PUT", "/settings", cfg107)
tv107 = call("POST", "/products", {"name": f"Split TV {suffix}", "marked_price": 100000, "cost_price": 70000})["result"]["id"]
call("POST", "/stock/receive", {"product_id": tv107, "quantity": 10, "cost_price": 70000})
on_hand107 = lambda: call("GET", f"/stock?q=Split TV {suffix}")["items"][0]["on_hand"]
m107 = "07" + str(int(time.time() * 11))[-8:]
# Give the customer points to redeem: a first sale earns 100.
first107 = call("POST", "/sales", {"customer": {"mobile": m107, "first_name": "Achieng"}, "items": [{"product_id": tv107, "quantity": 1, "unit_price": 100000}],
                                   "payment": {"method": "cash"}, "client_ref": str(uuid.uuid4())})["sale"]
c107 = first107["customer"]["id"]
code107 = "QS" + uuid.uuid4().hex[:8].upper()
split = lambda parts, extra=None, expect=200, **kw: call("POST", "/sales", {"customer_id": c107, "items": [{"product_id": tv107, "quantity": 1, "unit_price": 100000}],
                                                             "payment": {"method": "split"}, "payments": parts, "client_ref": str(uuid.uuid4()), **(extra or {})}, expect=expect, **kw)
r = split([{"method": "cash", "amount": 70000}, {"method": "mpesa", "amount": 40000, "reference": code107}], expect=422)
check("split: more than the payable is refused", "more than" in r["error"]["message"], r)
r = split([{"method": "cash", "amount": 20000}], expect=422)
check("split: an unpaid remainder needs to be allocated or put on credit", "still to be paid" in r["error"]["message"], r)
r = split([{"method": "cash", "amount": 50000}, {"method": "cash", "amount": 50000}], expect=422)
check("split: each method once per sale", "once" in r["error"]["message"], r)
before107 = on_hand107()
s107 = split([{"method": "cash", "amount": 20000}, {"method": "mpesa", "amount": 40000, "reference": code107}], {"redeem_points": 40})
sd107 = s107["sale"]
check("brief example: 40,000 points + 20,000 cash + 40,000 M-Pesa = 100,000; paid in full",
      float(sd107["redeemed_value"]) == 40000 and float(sd107["total"]) == 60000 and sd107["payment_method"] == "split" and sd107["payment_status"] == "paid", sd107)
check("each part recorded separately", sorted((p_["method"], float(p_["amount"])) for p_ in s107["payments"]) == [("cash", 20000.0), ("mpesa", 40000.0)], s107["payments"])
check("points earned only on the 60,000 not paid with points", sd107["points_earned"] == 60, sd107["points_earned"])
check("stock leaves once for the sale", on_hand107() == before107 - 1)
rcp107 = call("GET", f"/sales/{sd107['id']}/receipts")
snap107 = json.dumps(rcp107)
check("receipt lists both payments and the points", "Split payment" in snap107 and "M-Pesa" in snap107 and "Cash" in snap107, snap107[:300])
r = split([{"method": "mpesa", "amount": 100000, "reference": code107}], expect=422)
check("an M-Pesa code already used is refused", "already used" in r["error"].get("title", "") + r["error"]["message"], r)
# Partial payment, the rest on credit
before107 = on_hand107()
r = call("POST", "/sales", {"items": [{"product_id": tv107, "quantity": 1, "unit_price": 100000}], "payment": {"method": "split"},
                            "payments": [{"method": "cash", "amount": 30000}], "on_credit": True, "client_ref": str(uuid.uuid4())}, expect=422)
check("leaving a balance on credit needs a customer", "customer" in r["error"]["message"], r)
code107b = "QT" + uuid.uuid4().hex[:8].upper()
pc107 = split([{"method": "cash", "amount": 30000}, {"method": "mpesa", "amount": 20000, "reference": code107b}], {"on_credit": True, "due_date": (datetime.date.today() + datetime.timedelta(days=14)).isoformat()})
ps107 = pc107["sale"]
check("30,000 cash + 20,000 M-Pesa + 50,000 on credit: recorded as credit, partially paid",
      ps107["payment_method"] == "credit" and ps107["payment_status"] == "partially_paid" and float(ps107["amount_paid"]) == 50000, ps107)
check("the credit is linked to the sale with its 50,000 balance and both payments", pc107["credit"] and float(pc107["credit"]["balance"]) == 50000, pc107.get("credit"))
check("stock leaves once", on_hand107() == before107 - 1)
cr107 = pc107["credit"]["id"]
call("POST", f"/credit/{cr107}/payments", {"amount": 50000, "method": "cash"})
check("repaying the credit moves no stock", on_hand107() == before107 - 1)
check("repaid in full: the sale shows paid", call("GET", f"/sales/{ps107['id']}")["sale"]["payment_status"] == "paid")
pcoll = call("GET", "/reports/payments_collected?period=today")["rows"]
cash_row = next((x for x in pcoll if x["method"] == "cash"), {})
check("Payments Collected separates credit repayments from money taken at the sale", float(cash_row.get("repayments", 0)) >= 50000 and float(cash_row.get("at_sale", 0)) >= 50000, cash_row)
srep = call("GET", "/reports/sales?period=today")["rows"]
row107 = next(x for x in srep if x["receipt_no"] == sd107["receipt_no"])
check("Sales report: points value, paid and payment status", float(row107["points_value"]) == 40000 and row107["payment_status"] == "paid", row107)
# Offline: split payments need a connection
r = call("POST", "/sales", {"items": [{"product_id": tv107, "quantity": 1, "unit_price": 100000}], "payment": {"method": "split"},
                            "payments": [{"method": "cash", "amount": 100000}], "client_ref": str(uuid.uuid4()),
                            "offline_at": datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")}, expect=422)
check("split payments are refused offline", r["error"].get("title") == "Needs a connection" and "split payments" in r["error"]["message"], r)
# Return: refunded the way most of it was paid (M-Pesa 40,000 over cash 20,000)
it107 = s107["items"][0]["id"]
call("POST", f"/sales/{sd107['id']}/return", {"items": [{"sale_item_id": it107, "quantity": 1}], "reason": "Faulty", "restock": True})
refund107 = [p_ for p_ in call("GET", f"/sales/{sd107['id']}")["payments"] if float(p_["amount"]) < 0]
check("a returned split sale is refunded by its main method by default", refund107 and refund107[0]["method"] == "mpesa", refund107)
call("PUT", "/settings", orig107)

step("Roadmap 108: the page people land on after signing in")
cfg108 = call("GET", "/settings")["settings"]
call("PUT", "/settings", {**cfg108, "experience": {"landing": "/hack"}}, expect=400)
check("only known pages can be the landing page", True)
call("PUT", "/settings", {**cfg108, "experience": {"landing": "/pos"}})
check("business default saved: Record Sale", call("GET", "/auth/me")["settings"]["experience"]["landing"] == "/pos")
me108 = call("GET", "/auth/me")["user"]["preferences"]
call("PUT", "/auth/preferences", {**me108, "landing": "/nowhere"}, expect=400)
call("PUT", "/auth/preferences", {**me108, "landing": "/sales"})
check("personal choice saved (overrides the business default in the app)", call("GET", "/auth/me")["user"]["preferences"]["landing"] == "/sales")
call("PUT", "/auth/preferences", {**me108, "landing": ""})
call("PUT", "/settings", cfg108)

step("Roadmap 109: POS display and product list preferences")
cfg109 = call("GET", "/settings")["settings"]
check("defaults keep today's counter: compact cards, no images, prices shown, 2 / 4 columns",
      cfg109["sales"]["pos"] == {"style": "compact", "show_images": False, "tablet_columns": 2, "desktop_columns": 4, "show_prices": True}
      and cfg109["product"]["list_show_images"] is False and cfg109["product"]["list_show_prices"] is True, (cfg109["sales"]["pos"], cfg109["product"]))
for bad_, why in [({"style": "carousel"}, "unknown style"), ({"tablet_columns": 5}, "too many tablet columns"), ({"desktop_columns": 0}, "no desktop columns")]:
    call("PUT", "/settings", {**cfg109, "sales": {**cfg109["sales"], "pos": {**cfg109["sales"]["pos"], **bad_}}}, expect=400)
    check(f"POS display refused: {why}", True)
call("PUT", "/settings", {**cfg109, "sales": {**cfg109["sales"], "pos": {"style": "image", "show_images": True, "tablet_columns": 3, "desktop_columns": 5, "show_prices": False}},
                          "product": {**cfg109["product"], "list_show_images": True, "list_show_prices": False}})
me109 = call("GET", "/auth/me")["settings"]
check("saved and served to the app", me109["sales"]["pos"]["style"] == "image" and me109["sales"]["pos"]["show_prices"] is False and me109["product"]["list_show_images"] is True)
pp109 = call("GET", "/pos/products?all=true")
check("prices stay in the data for the cart and checkout (hiding is display only)", bool(pp109) and all(float(x["marked_price"]) >= 0 for x in pp109) and any(float(x["marked_price"]) > 0 for x in pp109))
call("PUT", "/settings", cfg109)

step("Roadmap 110: top-selling products first, by quantity, value or number of sales")
cfg110 = call("GET", "/settings")["settings"]
check("ranking on by default: quantity sold, this week", cfg110["sales"]["top_selling"] == {"enabled": True, "metric": "quantity", "period": "week"}, cfg110["sales"]["top_selling"])
call("PUT", "/settings", {**cfg110, "sales": {**cfg110["sales"], "top_selling": {"enabled": True, "metric": "units", "period": "week"}}}, expect=400)
check("unknown ranking metric refused", True)
rb110 = call("POST", "/branches", {"name": f"Ranking {suffix}", "code": f"RK{suffix}"})["id"]
mk110 = lambda n, price: call("POST", "/products", {"name": f"Rank {n} {suffix}", "marked_price": price, "cost_price": 1})["result"]["id"]
pa, pb, pc, pd = mk110("A", 100), mk110("B", 100), mk110("C", 10000), mk110("D", 100)
for pid in (pa, pb, pc, pd):
    call("POST", "/stock/receive", {"product_id": pid, "branch_id": rb110, "quantity": 50, "cost_price": 1}, branch=rb110)
sell110 = lambda pid, qty, price: call("POST", "/sales", {"items": [{"product_id": pid, "quantity": qty, "unit_price": price}], "payment": {"method": "cash"},
                                                            "client_ref": str(uuid.uuid4())}, branch=rb110)
sa = sell110(pa, 5, 100)                                   # A: 5 units, 1 sale, 500
for _ in range(3):
    sell110(pb, 1, 100)                                    # B: 3 units, 3 sales, 300
sell110(pc, 2, 10000)                                      # C: 2 units, 1 sale, 20,000
sd = sell110(pd, 10, 100)                                  # D: cancelled below — never counts
call("POST", f"/sales/{sd['sale']['id']}/cancel", {"reason": "Rung up by mistake"}, branch=rb110)
def ranks110(metric):
    call("PUT", "/settings", {**cfg110, "sales": {**cfg110["sales"], "top_selling": {"enabled": True, "metric": metric, "period": "week"}}})
    rows = call("GET", f"/pos/products?branch_id={rb110}&all=true", branch=rb110)
    return {x["id"]: x.get("rank") for x in rows if x["id"] in (pa, pb, pc, pd)}, [x["id"] for x in rows]
r, order = ranks110("quantity")
check("quantity sold: A (5) gold, B (3) silver, C (2) bronze; the cancelled sale never counts", (r[pa], r[pb], r[pc], r[pd]) == (1, 2, 3, None), r)
check("ranked products come first", order[:3] == [pa, pb, pc], order[:4])
r, _ = ranks110("sales")
check("number of completed sales: B (3 receipts) first", r[pb] == 1 and r[pd] is None, r)
r, _ = ranks110("value")
check("sales value: C (20,000) first, then A (500), then B (300)", (r[pc], r[pa], r[pb]) == (1, 2, 3), r)
it_a = sa["items"][0]["id"]
call("POST", f"/sales/{sa['sale']['id']}/return", {"items": [{"sale_item_id": it_a, "quantity": 4}], "reason": "Returned", "restock": True}, branch=rb110)
r, _ = ranks110("quantity")
check("returns count against a product (A now 1 unit: last)", (r[pb], r[pc], r[pa]) == (1, 2, 3), r)
srch = call("GET", f"/pos/products?branch_id={rb110}&all=true&q=Rank", branch=rb110)
check("searching keeps the search order (no ranking applied)", all(x.get("rank") is None for x in srch), [x.get("rank") for x in srch][:5])
call("PUT", "/settings", {**cfg110, "sales": {**cfg110["sales"], "top_selling": {"enabled": False, "metric": "quantity", "period": "week"}}})
off = call("GET", f"/pos/products?branch_id={rb110}&all=true", branch=rb110)
check("ranking off: the usual order, no stars", all(x.get("rank") is None for x in off))
call("PUT", "/settings", cfg110)

step("Roadmap 105: users are invited; they set their own PIN; step-up for Quick PIN sessions")
iv_roles = {r_["name"]: r_["id"] for r_ in call("GET", "/roles")}
iv_mail = f"invitee{suffix.lower()}@sshop.test"
iv = call("POST", "/users", {"name": f"Invitee {suffix}", "email": iv_mail, "phone": "0712 345 678", "role_id": iv_roles["Salesperson"], "all_branches": True, "branch_ids": []})
check("a user is created without any PIN and invited by email", iv["status"] == "invitation" and iv["email_status"] == "sent", iv)
row = lambda: next(u for u in call("GET", "/users") if u["id"] == iv["id"])
check("listed as Invitation sent, with when the link expires", row()["status"] == "invitation_sent" and row()["invite_expires_at"], row())
exp111 = datetime.datetime.fromisoformat(row()["invite_expires_at"].replace("Z", "+00:00")) - datetime.datetime.now(datetime.timezone.utc)
check("roadmap 111: the invitation link expires in 10 minutes", datetime.timedelta(minutes=8) < exp111 <= datetime.timedelta(minutes=10, seconds=30), exp111)
call("POST", "/users", {"name": "Again", "email": iv_mail.upper(), "role_id": iv_roles["Salesperson"], "all_branches": True, "branch_ids": []}, expect=400)
check("the same email cannot be invited twice (no duplicate account)", True)
call("POST", "/auth/login", {"email": iv_mail, "pin": "1234"}, token="none", branch="none", expect=400)
check("nobody can sign in before the person sets their PIN", True)
first_link = call("POST", f"/users/{iv['id']}/invite", {"send_email": False})["link"]
second_link = call("POST", f"/users/{iv['id']}/invite", {"send_email": False})["link"]
tok = lambda l: l.split("#t=")[1]
bad_ = urllib.request.Request(BASE + "/api/auth/link", data=json.dumps({"token": tok(first_link)}).encode(), method="POST", headers={"Content-Type": "application/json"})
try:
    urllib.request.urlopen(bad_)
    first_ok = True
except urllib.error.HTTPError:
    first_ok = False
check("a new activation link ends the earlier one (single use, one at a time)", not first_ok)
old111 = call("POST", "/auth/link", {"token": tok(first_link)}, token="none", branch="none", expect=422)
check("a replaced invitation says it has expired and to ask for a new one", old111["error"]["title"] == "Invitation expired" and "administrator" in old111["error"]["message"], old111)
info = call("POST", "/auth/link", {"token": tok(second_link)}, token="none", branch="none")
check("the link opens the set-up page (email masked) and the user shows Activation pending", info["kind"] == "invite" and iv_mail not in json.dumps(info) and row()["status"] == "activation_pending", (info, row()["status"]))
call("POST", "/auth/set-pin", {"token": tok(second_link), "pin": "8642"}, token="none", branch="none")
reuse = urllib.request.Request(BASE + "/api/auth/set-pin", data=json.dumps({"token": tok(second_link), "pin": "9999"}).encode(), method="POST", headers={"Content-Type": "application/json"})
try:
    urllib.request.urlopen(reuse)
    reused = True
except urllib.error.HTTPError:
    reused = False
check("the set-up link works once", not reused)
used111 = call("POST", "/auth/link", {"token": tok(second_link)}, token="none", branch="none", expect=422)
check("a used invitation says so", used111["error"]["title"] == "Invitation already used", used111)
ivl = call("POST", "/auth/login", {"email": iv_mail, "pin": "8642"}, token="none", branch="none")
check("signs in with their own PIN; account Active; Quick PIN offered once", row()["status"] == "active" and ivl["profile"]["user"]["quick_pin_offer"] is True and not ivl["profile"]["user"]["must_change_pin"], ivl["profile"]["user"])
IV = {"token": ivl["token"], "branch": BRANCH}
call("POST", "/auth/quick-pin/offer/dismiss", {}, **IV)
check("Maybe later: not offered again", call("GET", "/auth/me", **IV)["user"]["quick_pin_offer"] is False)
r = call("POST", f"/users/{iv['id']}/invite", {"send_email": True}, expect=422)
check("an account already set up is not invited again", "already set up" in r["error"]["message"], r)
iv_audit = call("GET", "/audit?period=today&limit=300&module=users")["items"]
check("invitations audited, never a link or PIN", any(a["action"] == "invite_link_created" for a in iv_audit) and tok(second_link) not in json.dumps(iv_audit) and not mentions_word(iv_audit, 8642))
# Step-up: a Quick PIN session asked for a full sign-in continues after the full PIN is confirmed.
su_mail = f"stepup{suffix.lower()}@sshop.test"
call("POST", "/users", {"name": f"Step Up {suffix}", "email": su_mail, "pin": "4321", "role_id": iv_roles["Tenant Administrator"], "all_branches": True, "branch_ids": []})
SF = {"token": call("POST", "/auth/login", {"email": su_mail, "pin": "4321"})["token"], "branch": BRANCH}
su_dev = call("PUT", "/auth/quick-pin", {"current_pin": "4321", "quick_pin": "2580", "device_name": "Office PC"}, **SF)
SQ = {"token": call("POST", "/auth/quick-login", {"device_token": su_dev["device_token"], "pin": "2580"}, token="none", branch="none")["token"], "branch": BRANCH}
body_ = {"name": f"Via Step Up {suffix}", "email": f"viastepup{suffix.lower()}@sshop.test", "role_id": iv_roles["Salesperson"], "all_branches": True, "branch_ids": []}
r = call("POST", "/users", body_, **SQ, expect=422)
check("creating a user from a Quick PIN session asks for a full sign-in (nothing created)", r["error"]["title"] == "Full sign-in needed" and not any(u["email"] == body_["email"] for u in call("GET", "/users")), r)
r = call("POST", "/auth/step-up", {"pin": "2580"}, **SQ, expect=400)
check("the Quick PIN is not accepted as the full sign-in", True)
up = call("POST", "/auth/step-up", {"pin": "4321"}, **SQ)
SU = {"token": up["token"], "branch": BRANCH}
check("full PIN confirmed: a full session; the Quick one ends", call("GET", "/auth/me", **SU)["quick"] is False)
call("GET", "/auth/me", **SQ, expect=401)
made = call("POST", "/users", body_, **SU)
check("the same request then succeeds once (no duplicate)", made["status"] == "invitation" and sum(1 for u in call("GET", "/users") if u["email"] == body_["email"]) == 1)

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
