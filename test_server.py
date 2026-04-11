from http.server import BaseHTTPRequestHandler, HTTPServer
import time

request_times = []

class MetricsHandler(BaseHTTPRequestHandler):
    def do_GET(self):
        global request_times
        current_time = time.time()
        # Keep requests within the last 1 second
        request_times = [t for t in request_times if current_time - t < 1.0]

        # WAF Rate Limit simulation (max 5 requests per second)
        if len(request_times) > 5:
            self.send_response(429)
            self.end_headers()
            self.wfile.write(b"Rate Limited by WAF!")
            return

        request_times.append(current_time)

        # WAF Folder permissions simulation
        if "admin" in self.path or "hidden" in self.path:
            self.send_response(403)
            self.end_headers()
            self.wfile.write(b"Forbidden")
        elif "api" in self.path or "public" in self.path:
            self.send_response(200)
            self.end_headers()
            self.wfile.write(b'{"status": "ok"}')
        else:
            self.send_response(404)
            self.end_headers()
            self.wfile.write(b"Not Found")

    def log_message(self, format, *args):
        pass # Hide HTTP server logs so we can see Sigma's logs cleanly 

if __name__ == '__main__':
    server = HTTPServer(('127.0.0.1', 8080), MetricsHandler)
    print("Starting WAF Test Server on http://127.0.0.1:8080")
    server.serve_forever()
