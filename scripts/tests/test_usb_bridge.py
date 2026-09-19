import importlib.util
from pathlib import Path
import http.client
import json
import threading
import unittest

spec = importlib.util.spec_from_file_location('bridge', Path(__file__).parents[1] / 'usb-bridge.py')
bridge = importlib.util.module_from_spec(spec)
spec.loader.exec_module(bridge)

class BridgeBoundary(unittest.TestCase):
    def setUp(self):
        self.server = bridge.HTTPServer(('127.0.0.1', 0), bridge.make_handler('/nonexistent', 'test-token'))
        self.thread = threading.Thread(target=self.server.serve_forever, daemon=True)
        self.thread.start()

    def tearDown(self):
        self.server.shutdown()
        self.server.server_close()
        self.thread.join()

    def post(self, value, token='test-token'):
        c = http.client.HTTPConnection('127.0.0.1', self.server.server_port, timeout=3)
        try:
            c.request('POST', '/rpc', json.dumps(value), {'Authorization': 'Bearer ' + token})
            response = c.getresponse()
            status = response.status
            response.read()
            return status
        finally:
            c.close()

    def test_no_capability_cannot_reach_keeper(self):
        self.assertEqual(self.post({'version': 1, 'operation': {'type': 'screen'}}, 'wrong'), 403)

    def test_stop_and_session_selection_cannot_expand_capability(self):
        self.assertEqual(self.post({'version': 1, 'operation': {'type': 'stop'}}), 400)
        self.assertEqual(self.post({'version': 1, 'operation': {'type': 'new', 'session': 'other'}}), 400)

    def test_malformed_shapes_do_not_kill_bridge(self):
        self.assertEqual(self.post([]), 400)
        self.assertEqual(self.post({'operation': []}), 400)
        self.assertEqual(self.post({}, 'wrong'), 403)

    def test_oversized_request_is_rejected(self):
        # Rejection must happen from the header, before reading/allocating a body.
        # Sending the entire body races the intentional early close on macOS.
        c = http.client.HTTPConnection('127.0.0.1', self.server.server_port, timeout=3)
        try:
            c.putrequest('POST', '/rpc')
            c.putheader('Authorization', 'Bearer test-token')
            c.putheader('Content-Length', str(bridge.LIMIT + 1))
            c.endheaders()
            response = c.getresponse()
            self.assertEqual(response.status, 400)
            response.read()
        finally:
            c.close()

if __name__ == '__main__':
    unittest.main()
