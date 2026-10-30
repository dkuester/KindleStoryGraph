#!/usr/bin/env python3
"""Fake StoryGraph for the end-to-end tests (tests/e2e.rs, docs/emulator.md).

A stateful, deterministic stand-in for the parts of app.thestorygraph.com the
client in src/storygraph.rs talks to. Python stdlib only, no external deps.

The HTML comes from tests/e2e/templates/ and mirrors what the client's
parsers expect — it is NOT captured from StoryGraph. A green e2e run proves
the client and this fake agree; whether the real site still serves that
markup is only checked on the device (see docs/emulator.md, "Drift").

What it enforces, so the client's session handling is exercised:
  - a valid `_storygraph_session` cookie (rotated on every response, like
    Rails) or `remember_user_token`; otherwise 302 to /users/sign_in
  - the CSRF token (form field `authenticity_token` or `X-CSRF-Token`) and,
    when sent, an `Origin` matching this server on every POST; else 422

Control channel (not part of StoryGraph):
  GET  /__state   JSON: shelf, tags, events (every mutation), requests
  POST /__reset   JSON {"shelf": {id: {"status": slug, "percent": n}},
                        "tags": [id, ...]}: replaces the shelf, clears the
                  rest (faults, events, request log, session expiry)
  POST /__fault   JSON {"path": prefix, "method": "GET"|"POST"|null,
                        "kind": "500"|"slow"|"expired"|"garbage",
                        "times": n|null, "delay": seconds}
                  "expired" with path "" expires every session instead.
  POST /__clear_faults   drops all faults and the session expiry

Usage: fake_storygraph.py [--host 127.0.0.1] [--port 0]
Prints "listening on http://HOST:PORT" (flushed) once ready.
"""

import argparse
import html
import json
import os
import re
import socketserver
import sys
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from string import Template
from urllib.parse import parse_qs, urlsplit

HERE = os.path.dirname(os.path.abspath(__file__))

# Fixed fake credentials; the e2e config uses the same values.
SESSION_COOKIE = "fake-session-cookie"
REMEMBER_TOKEN = "fake-remember-token"
CSRF_TOKEN = "fake-csrf-token-4f1c"
USER_SLUG = "e2e-reader"
LOGIN_EMAIL = "reader@example.test"
LOGIN_PASSWORD = "correct horse battery staple"

STATUSES = {
    "to-read": "To read",
    "currently-reading": "Currently reading",
    "read": "Read",
    "paused": "Paused",
    "did-not-finish": "Did not finish",
    "rereading": "Rereading",
}
# Statuses whose page shows the edit-progress bar.
PROGRESS_STATUSES = {"currently-reading", "rereading", "paused"}


def load_template(name):
    with open(os.path.join(HERE, "templates", name), encoding="utf-8") as f:
        return Template(f.read())


TEMPLATES = {
    n: load_template(n + ".html")
    for n in ("book", "status_block", "browse", "search_result", "home", "sign_in")
}

with open(os.path.join(HERE, "catalog.json"), encoding="utf-8") as f:
    CATALOG = json.load(f)["books"]
BOOKS = {b["id"]: b for b in CATALOG}


def slugify(s):
    return re.sub(r"[^a-z0-9]+", "-", s.lower()).strip("-")


def tokens(s):
    return {w for w in re.split(r"\W+", s.lower()) if w}


def esc(v):
    return html.escape(str(v), quote=True)


class State:
    def __init__(self):
        self.lock = threading.Lock()
        self.reset({})

    def reset(self, spec):
        self.shelf = {
            bid: {"status": e["status"], "percent": int(e.get("percent", 0))}
            for bid, e in spec.get("shelf", {}).items()
        }
        self.tags = set(spec.get("tags", []))
        self.faults = []
        self.events = []
        self.requests = []
        self.expired = False
        self.sessions = {SESSION_COOKIE}
        self.session_counter = 0

    def snapshot(self):
        return {
            "shelf": self.shelf,
            "tags": sorted(self.tags),
            "events": self.events,
            "requests": self.requests,
            "expired": self.expired,
        }

    def take_fault(self, method, path):
        """Returns the first matching fault (consuming one use), or None."""
        for f in self.faults:
            if f.get("method") and f["method"] != method:
                continue
            if not path.startswith(f.get("path", "")):
                continue
            if f.get("times") is not None:
                if f["times"] <= 0:
                    continue
                f["times"] -= 1
            return f
        return None


STATE = State()


class Handler(BaseHTTPRequestHandler):
    server_version = "FakeStoryGraph/1"
    protocol_version = "HTTP/1.1"

    def log_message(self, fmt, *args):
        if os.environ.get("FAKE_STORYGRAPH_VERBOSE"):
            sys.stderr.write("fake-storygraph: " + fmt % args + "\n")

    # ── plumbing ────────────────────────────────────────────────────────────

    def send(self, status, body=b"", ctype="text/html; charset=utf-8", headers=()):
        if isinstance(body, str):
            body = body.encode("utf-8")
        self.send_response(status)
        self.send_header("Content-Type", ctype)
        self.send_header("Content-Length", str(len(body)))
        for k, v in headers:
            self.send_header(k, v)
        self.end_headers()
        if self.command != "HEAD":
            self.wfile.write(body)

    def redirect(self, location, headers=()):
        self.send(302, b"", headers=[("Location", location), *headers])

    def cookies(self):
        out = {}
        for part in self.headers.get("Cookie", "").split(";"):
            if "=" in part:
                k, v = part.strip().split("=", 1)
                out[k] = v
        return out

    def body(self):
        n = int(self.headers.get("Content-Length") or 0)
        return self.rfile.read(n) if n else b""

    def authenticated(self):
        if STATE.expired:
            return False
        c = self.cookies()
        return (
            c.get("_storygraph_session") in STATE.sessions
            or c.get("remember_user_token") == REMEMBER_TOKEN
        )

    def rotate_session(self):
        """Like Rails: every authenticated response carries a fresh session
        cookie; previously issued ones stay valid."""
        STATE.session_counter += 1
        sid = f"fake-session-{STATE.session_counter}"
        STATE.sessions.add(sid)
        return [("Set-Cookie", f"_storygraph_session={sid}; path=/; HttpOnly; SameSite=Lax")]

    def page(self, template, **values):
        values.setdefault("csrf", CSRF_TOKEN)
        values.setdefault("user_slug", USER_SLUG)
        self.send(200, TEMPLATES[template].substitute(values), headers=self.rotate_session())

    # ── dispatch ────────────────────────────────────────────────────────────

    def do_GET(self):
        self.dispatch("GET")

    def do_POST(self):
        self.dispatch("POST")

    def dispatch(self, method):
        url = urlsplit(self.path)
        path, query = url.path, parse_qs(url.query)
        raw = self.body() if method == "POST" else b""

        if path.startswith("/__"):
            return self.control(method, path, raw)

        with STATE.lock:
            STATE.requests.append({"method": method, "path": path, "query": url.query})
            fault = STATE.take_fault(method, path)
        if fault:
            kind = fault["kind"]
            if kind == "500":
                return self.send(500, "<html><body>We're sorry, but something went wrong.</body></html>")
            if kind == "slow":
                time.sleep(float(fault.get("delay", 5)))
            elif kind == "expired":
                return self.redirect("/users/sign_in")
            elif kind == "garbage":
                return self.send(200, "<html><body><h1>Down for maintenance</h1></body></html>")

        if path == "/users/sign_in":
            return self.sign_in(method, raw)

        if not self.authenticated():
            return self.redirect("/users/sign_in")

        with STATE.lock:
            if method == "GET":
                return self.get(path, query)
            return self.post(path, query, raw)

    # ── control channel ─────────────────────────────────────────────────────

    def control(self, method, path, raw):
        with STATE.lock:
            if method == "GET" and path == "/__state":
                return self.send(200, json.dumps(STATE.snapshot()), "application/json")
            if method == "POST" and path == "/__reset":
                STATE.reset(json.loads(raw or b"{}"))
                return self.send(200, b"{}", "application/json")
            if method == "POST" and path == "/__fault":
                f = json.loads(raw)
                if f["kind"] == "expired" and not f.get("path"):
                    STATE.expired = True
                else:
                    STATE.faults.append(f)
                return self.send(200, b"{}", "application/json")
            if method == "POST" and path == "/__clear_faults":
                STATE.faults = []
                STATE.expired = False
                return self.send(200, b"{}", "application/json")
        self.send(404, "unknown control endpoint", "text/plain")

    # ── StoryGraph endpoints ────────────────────────────────────────────────

    def sign_in(self, method, raw):
        if method == "GET":
            return self.send(200, TEMPLATES["sign_in"].substitute(csrf=CSRF_TOKEN))
        form = {k: v[0] for k, v in parse_qs(raw.decode()).items()}
        if form.get("authenticity_token") != CSRF_TOKEN:
            return self.send(422, "InvalidAuthenticityToken", "text/plain")
        if form.get("user[email]") != LOGIN_EMAIL or form.get("user[password]") != LOGIN_PASSWORD:
            return self.send(200, TEMPLATES["sign_in"].substitute(csrf=CSRF_TOKEN))
        with STATE.lock:
            STATE.expired = False
            headers = self.rotate_session()
        headers.append(("Set-Cookie", f"remember_user_token={REMEMBER_TOKEN}; path=/; HttpOnly"))
        self.redirect("/", headers)

    def get(self, path, query):
        if path == "/":
            return self.page("home")
        if path == "/browse":
            return self.browse(query.get("search_term", [""])[0])
        m = re.fullmatch(r"/books/([^/]+)/check_tags", path)
        if m and m.group(1) in BOOKS:
            body = json.dumps({"has_tags": m.group(1) in STATE.tags})
            return self.send(200, body, "application/json", self.rotate_session())
        m = re.fullmatch(r"/books/([^/]+)", path)
        if m and m.group(1) in BOOKS:
            return self.book(BOOKS[m.group(1)])
        self.send(404, "<html><body>Not found</body></html>")

    def browse(self, term):
        want = tokens(term)
        hits = [b for b in CATALOG if want and want <= tokens(b["title"] + " " + b["author"])]
        results = "\n".join(
            TEMPLATES["search_result"].substitute(
                id=esc(b["id"]),
                title=esc(b["title"]),
                author=esc(b["author"]),
                author_slug=slugify(b["author"]),
                pages=b["pages"],
                format=esc(b["format"]),
            )
            for b in hits
        )
        self.page("browse", query=esc(term), results=results)

    def book(self, b):
        entry = STATE.shelf.get(b["id"])
        status_block = ""
        if entry:
            pct = entry["percent"]
            bar = ""
            if entry["status"] in PROGRESS_STATUSES:
                bar = (
                    '<div class="progress-bar edit-progress">'
                    f'<div class="progress" style="width: {pct}%"></div></div>'
                )
            status_block = TEMPLATES["status_block"].substitute(
                status_label=STATUSES[entry["status"]],
                progress_bar=bar,
                percent=pct,
                pages_read=b["pages"] * pct // 100,
            )
        self.page(
            "book",
            title=esc(b["title"]),
            author=esc(b["author"]),
            author_slug=slugify(b["author"]),
            pages=b["pages"],
            format=esc(b["format"]),
            status_block=status_block,
        )

    def csrf_ok(self, form):
        token = form.get("authenticity_token") or self.headers.get("X-CSRF-Token")
        origin = self.headers.get("Origin")
        own = f"http://{self.headers.get('Host')}"
        return token == CSRF_TOKEN and (origin is None or origin == own)

    def post(self, path, query, raw):
        form = {k: v[0] for k, v in parse_qs(raw.decode()).items()}
        if not self.csrf_ok(form):
            return self.send(422, "ActionController::InvalidAuthenticityToken", "text/plain")

        if path == "/update-status.js":
            bid = query.get("book_id", [""])[0]
            status = query.get("status", [""])[0]
            if bid not in BOOKS or status not in STATUSES:
                return self.send(422, "bad book_id/status", "text/plain")
            entry = STATE.shelf.setdefault(bid, {"status": status, "percent": 0})
            entry["status"] = status
            if status == "read":
                entry["percent"] = 100
            elif status == "to-read":
                entry["percent"] = 0
            STATE.events.append({"book": bid, "status": status})
            return self.send(200, "// status updated", "text/javascript", self.rotate_session())

        if path == "/update-progress":
            bid = form.get("book_id", "")
            if bid not in BOOKS:
                return self.send(422, "unknown book", "text/plain")
            if bid not in STATE.shelf:
                # The client has to set a status first.
                return self.send(422, "book is not on a shelf", "text/plain")
            try:
                value = int(form.get("read_status[progress_number]", ""))
            except ValueError:
                return self.send(422, "bad progress_number", "text/plain")
            ptype = form.get("read_status[progress_type]")
            if ptype != "percentage" or not 0 <= value <= 100:
                return self.send(422, "bad progress", "text/plain")
            if form.get("read_status[book_num_of_pages]") != str(BOOKS[bid]["pages"]):
                return self.send(422, "book_num_of_pages mismatch", "text/plain")
            STATE.shelf[bid]["percent"] = value
            STATE.events.append({"book": bid, "progress": value})
            # StoryGraph answers with a redirect back to the book page.
            return self.redirect(f"/books/{bid}", self.rotate_session())

        m = re.fullmatch(r"/remove-book/([^/]+)", path)
        if m and m.group(1) in BOOKS:
            bid = m.group(1)
            remove_tags = query.get("remove_tags", [""])[0]
            if remove_tags not in ("true", "false"):
                return self.send(422, "remove_tags must be true/false", "text/plain")
            STATE.shelf.pop(bid, None)
            if remove_tags == "true":
                STATE.tags.discard(bid)
            STATE.events.append({"book": bid, "removed": True, "remove_tags": remove_tags == "true"})
            return self.send(200, "// removed", "text/javascript", self.rotate_session())

        self.send(404, "not found", "text/plain")


class Server(ThreadingHTTPServer):
    daemon_threads = True

    def server_bind(self):
        # HTTPServer.server_bind resolves socket.getfqdn(host): a reverse DNS
        # lookup that can stall for minutes without network (CI sandboxes,
        # the emulator's network-less container). The name is never used.
        socketserver.TCPServer.server_bind(self)
        self.server_name, self.server_port = self.server_address[:2]


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n", 1)[0])
    ap.add_argument("--host", default="127.0.0.1")
    ap.add_argument("--port", type=int, default=0)
    args = ap.parse_args()
    srv = Server((args.host, args.port), Handler)
    print(f"listening on http://{args.host}:{srv.server_address[1]}", flush=True)
    try:
        srv.serve_forever()
    except KeyboardInterrupt:
        pass


if __name__ == "__main__":
    main()
