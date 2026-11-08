import http.client
import threading
import unittest
from http.server import ThreadingHTTPServer

from fixture_server import Fixture, PAGE


class FixtureTests(unittest.TestCase):
    def setUp(self):
        self.server = ThreadingHTTPServer(("127.0.0.1", 0), Fixture)
        self.thread = threading.Thread(target=self.server.serve_forever, daemon=True)
        self.thread.start()

    def tearDown(self):
        self.server.shutdown()
        self.server.server_close()
        self.thread.join(timeout=2)

    def request(self, method, path):
        connection = http.client.HTTPConnection(*self.server.server_address, timeout=2)
        try:
            connection.request(method, path)
            response = connection.getresponse()
            return response.status, dict(response.getheaders()), response.read()
        finally:
            connection.close()

    def test_only_exact_fixture_is_served_without_network_callbacks(self):
        status, headers, body = self.request("GET", "/")
        self.assertEqual(status, 200)
        self.assertEqual(body, PAGE)
        self.assertIn("connect-src 'none'", headers["Content-Security-Policy"])
        self.assertEqual(headers["Cache-Control"], "no-store")

    def test_paths_queries_and_write_methods_never_expose_host_files_or_actions(self):
        for path in ["/../../AGENTS.md", "/fixture.html", "/?file=C:/secret", "/ipc", "/favicon.ico"]:
            self.assertEqual(self.request("GET", path)[0], 404)
        self.assertEqual(self.request("POST", "/")[0], 501)


if __name__ == "__main__":
    unittest.main()
