from __future__ import annotations

import os
import traceback
from typing import TYPE_CHECKING

from pyqwest import Client

from pyvoy.asgi.httpclient import HTTPTransport

if TYPE_CHECKING:
    from asgiref.typing import ASGIReceiveCallable, ASGISendCallable, Scope

client = Client(HTTPTransport())
backend = os.getenv("TEST_URL")


async def client_get() -> None:
    url = f"{backend}/echo"
    resp = await client.get(url, params={"foo": "bar"})
    assert resp.status == 200
    assert resp.headers["x-echo-method"] == "GET"
    assert resp.headers["x-echo-query-string"] == "foo=bar"
    assert resp.content == b""
    assert len(resp.trailers) == 0


async def app(
    scope: Scope, _receive: ASGIReceiveCallable, send: ASGISendCallable
) -> None:
    assert scope["type"] == "http"  # noqa: S101
    headers = {k.decode(): v.decode() for k, v in scope["headers"]}
    try:
        match headers["x-test-case"]:
            case "client_get":
                await client_get()
            case _:
                msg = f"Unknown test case: {headers['x-test-case']}"
                raise RuntimeError(msg)  # noqa: TRY301
    except Exception:
        response_body = traceback.format_exc().encode()
        print(response_body.decode())
        await send(
            {
                "type": "http.response.start",
                "status": 500,
                "headers": [
                    (b"content-type", b"text/plain"),
                    (b"content-length", str(len(response_body)).encode()),
                ],
                "trailers": False,
            }
        )
        await send(
            {"type": "http.response.body", "body": response_body, "more_body": False}
        )
        return

    await send(
        {"type": "http.response.start", "status": 200, "headers": [], "trailers": False}
    )
    await send({"type": "http.response.body", "body": b"", "more_body": False})
