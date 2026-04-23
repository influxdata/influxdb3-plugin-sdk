"""Plugin entry point for the `process_request` trigger."""


def process_request(influxdb3_local, query_params, request_headers, request_body, args):
    """Called on each incoming request. Returns a Flask-style tuple or a bare value."""
    return {"ok": True}
