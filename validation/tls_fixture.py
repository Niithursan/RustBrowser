"""Loopback-only untrusted TLS fixture. Never installs a certificate or logs requests."""
import argparse
import http.server
import pathlib
import ssl

class Handler(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        self.send_response(200)
        self.send_header('Content-Length', '0')
        self.end_headers()

    def log_message(self, *args):
        pass

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--root', type=pathlib.Path, required=True)
    args = parser.parse_args()
    root = args.root.resolve(strict=True)
    if not (root / '.rust-browser-daily-validation').is_file():
        raise SystemExit('Owned fixture marker is required')
    context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
    context.minimum_version = ssl.TLSVersion.TLSv1_2
    context.load_cert_chain(root / 'tls-cert.pem', root / 'tls-key.pem')
    with http.server.HTTPServer(('127.0.0.1', 0), Handler) as server:
        server.socket = context.wrap_socket(server.socket, server_side=True)
        with (root / 'tls-port.txt').open('x', encoding='ascii') as port:
            port.write(str(server.server_port))
        server.serve_forever()

if __name__ == '__main__':
    main()
