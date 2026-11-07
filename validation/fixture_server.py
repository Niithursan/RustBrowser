"""Loopback-only, synthetic compatibility fixture. Never use account data or real files."""
import argparse
import io
import math
import struct
import time
import wave
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

PAGE = b"""<!doctype html><html lang="en"><meta charset="utf-8"><title>Rust Browser compatibility fixture</title>
<style>body{font:18px system-ui;max-width:1000px;margin:30px auto;padding:20px}button,a,input{font:inherit;margin:8px;padding:10px}video{width:420px;height:240px;background:#234}#status{padding:16px;background:#def}</style>
<h1>Controlled compatibility fixture</h1><p>No accounts, credentials, camera or microphone are needed. Upload only the supplied synthetic text fixture.</p>
<button onclick="window.auth=window.open('/auth','fixture-auth')">Open synthetic login popup</button><span id="auth-state">Not completed</span>
<p><a target="_blank" rel="noopener" href="/">New isolated tab</a> <a href="/redirect">HTTP redirect</a> <a href="/download">Small download</a> <a href="/slow">Slow download</a></p>
<form method="post" action="/upload" enctype="multipart/form-data"><label>Synthetic file upload <input type="file" name="fixture" required></label><button>Upload fixture</button></form>
<button onclick="sound.play().then(()=>statusText('Audio playing')).catch(()=>statusText('Audio unavailable'))">Play quiet fixture audio</button><button onclick="sound.pause();statusText('Audio stopped')">Stop audio</button>
<p><button onclick="startVideo()">Play synthetic video</button><button onclick="video.requestFullscreen()">Fullscreen video</button><button onclick="startRtc()">Start local WebRTC loopback</button><button onclick="stopMedia()">Stop video and loopback</button></p>
<video id="video" muted controls playsinline></video><video id="remote" muted controls playsinline></video><canvas id="canvas" width="420" height="240" hidden></canvas><div role="status" id="status">Ready</div>
<script>
const sound=new Audio('/audio.wav');sound.loop=true;sound.volume=.05;
let stream,timer,peers=[];const statusText=s=>document.getElementById('status').textContent=s;
addEventListener('message',e=>{if(e.origin===location.origin&&e.data==='fixture-login-complete')document.getElementById('auth-state').textContent='Popup message received; cookie: '+document.cookie.includes('fixture_session=joined')});
async function startVideo(){if(!stream){let tick=0;const c=document.getElementById('canvas'),ctx=c.getContext('2d');timer=setInterval(()=>{ctx.fillStyle=tick%2?'#286':'#528';ctx.fillRect(0,0,420,240);ctx.fillStyle='white';ctx.font='24px system-ui';ctx.fillText('Synthetic frame '+(++tick),25,100)},300);stream=c.captureStream(5)}video.srcObject=stream;await video.play();statusText('Synthetic video playing. Use Keep awake for background media/RTC tests.')}
async function startRtc(){await startVideo();const a=new RTCPeerConnection({iceServers:[]}),b=new RTCPeerConnection({iceServers:[]});peers.push(a,b);a.onicecandidate=e=>{if(e.candidate)b.addIceCandidate(e.candidate).catch(()=>statusText('Loopback ICE failed'))};b.onicecandidate=e=>{if(e.candidate)a.addIceCandidate(e.candidate).catch(()=>statusText('Loopback ICE failed'))};b.ontrack=e=>{remote.srcObject=e.streams[0];remote.play()};a.onconnectionstatechange=()=>statusText('Local WebRTC: '+a.connectionState);stream.getTracks().forEach(t=>a.addTrack(t,stream));await a.setLocalDescription(await a.createOffer());await b.setRemoteDescription(a.localDescription);await b.setLocalDescription(await b.createAnswer());await a.setRemoteDescription(b.localDescription)}
function stopMedia(){sound.pause();peers.forEach(p=>p.close());peers=[];if(stream)stream.getTracks().forEach(t=>t.stop());stream=null;clearInterval(timer);video.srcObject=null;remote.srcObject=null;statusText('Media stopped')}
</script></html>"""
AUTH = b"""<!doctype html><html lang="en"><title>Synthetic authentication popup</title><h1>Synthetic authentication</h1><p>This fixture does not authenticate any real account.</p><button onclick="document.cookie='fixture_session=joined; SameSite=Lax; path=/';if(opener)opener.postMessage('fixture-login-complete',location.origin);window.close()">Complete fixture login and close</button></html>"""


def audio():
    data = io.BytesIO()
    with wave.open(data, "wb") as output:
        output.setparams((1, 2, 8000, 8000, "NONE", "not compressed"))
        output.writeframes(b"".join(struct.pack("<h", int(math.sin(i * math.tau * 440 / 8000) * 200)) for i in range(8000)))
    return data.getvalue()


AUDIO = audio()


class Handler(BaseHTTPRequestHandler):
    def log_message(self, *_):
        pass  # No URLs, uploaded filenames or bodies are logged.

    def send(self, body, kind="text/html; charset=utf-8", status=200, headers=()):
        self.send_response(status)
        self.send_header("Content-Type", kind)
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Cache-Control", "no-store")
        for name, value in headers:
            self.send_header(name, value)
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self):
        path = self.path.split("?", 1)[0]
        try:
            if path in ("/", "/index.html"):
                self.send(PAGE)
            elif path == "/auth":
                self.send(AUTH)
            elif path == "/audio.wav":
                self.send(AUDIO, "audio/wav")
            elif path == "/redirect":
                self.send(b"", status=302, headers=(("Location", "/"),))
            elif path == "/download":
                self.send(b"Synthetic safe download fixture.\n" * 2048, "application/octet-stream", headers=(("Content-Disposition", 'attachment; filename="fixture.txt"'),))
            elif path == "/slow":
                size = 4 * 1024 * 1024
                self.send_response(200)
                self.send_header("Content-Length", str(size))
                self.send_header("Content-Type", "application/octet-stream")
                self.send_header("Content-Disposition", 'attachment; filename="slow-fixture.txt"')
                self.end_headers()
                for _ in range(size // 16384):
                    self.wfile.write(b"Synthetic data. " * 1024)
                    self.wfile.flush()
                    time.sleep(.2)
            else:
                self.send(b"Fixture not found", status=404)
        except (BrokenPipeError, ConnectionResetError):
            pass

    def do_POST(self):
        length = int(self.headers.get("Content-Length", "0"))
        if self.path != "/upload" or not 0 < length <= 1024 * 1024:
            self.send(b"Only small synthetic fixture uploads are supported", status=413)
            return
        self.rfile.read(length)  # Discard; never save or reflect file contents.
        self.send(f"<!doctype html><title>Upload received</title><h1>Received {length} fixture bytes</h1><a href='/'>Back to fixture</a>".encode())


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--port", type=int, default=8769)
    args = parser.parse_args()
    ThreadingHTTPServer(("127.0.0.1", args.port), Handler).serve_forever()
