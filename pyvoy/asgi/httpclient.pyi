from collections.abc import Awaitable

from pyqwest import Request, Response

class HTTPTransport:
    def execute(self, request: Request) -> Awaitable[Response]: ...
