"""
Dictando – system-wide voice dictation for Windows.
Uses Gemini 2.0 Flash for fast cloud speech-to-text.
"""
import os, sys, json, logging, threading, time, io, struct
import http.server, socketserver, hashlib, secrets, webbrowser
import urllib.parse, urllib.request

logging.basicConfig(level=logging.INFO, format='%(asctime)s %(levelname)s %(message)s')
logger = logging.getLogger("dictando")

CHUNK = 1024
CHANNELS = 1
RATE = 16000
TAIL_BUFFER_S = 0.3   # extra recording after key release to capture trailing words

GOOGLE_CLIENT_ID = "987257492661-dr4orf873kn2nj2h31hur9mc6tfj9cio.apps.googleusercontent.com"
GOOGLE_CLIENT_SECRET = "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"
GOOGLE_AUTH_ENDPOINT = "https://accounts.google.com/o/oauth2/v2/auth"
GOOGLE_TOKEN_ENDPOINT = "https://oauth2.googleapis.com/token"

SETTINGS_FILE = os.path.join(os.path.expanduser("~"), ".dictando_settings.json")
GEMINI_MODEL = "gemini-2.0-flash"


def load_local_settings():
    try:
        if os.path.exists(SETTINGS_FILE):
            with open(SETTINGS_FILE, 'r') as f:
                return json.load(f)
    except:
        pass
    return {}

def save_local_settings(settings):
    try:
        with open(SETTINGS_FILE, 'w') as f:
            json.dump(settings, f, indent=2)
    except:
        pass


class OAuthCallbackHandler(http.server.BaseHTTPRequestHandler):
    def log_message(self, *a): pass
    def do_GET(self):
        params = urllib.parse.parse_qs(urllib.parse.urlparse(self.path).query)
        if 'code' in params:
            self.server.auth_code = params['code'][0]
            self.send_response(200); self.send_header('Content-Type','text/html'); self.end_headers()
            self.wfile.write(b'<html><body style="font-family:system-ui;display:flex;align-items:center;justify-content:center;height:100vh;margin:0;background:#1a1a1a;color:#e5e5e5"><div style="text-align:center"><h1 style="color:#22c55e">&#10003; Signed in!</h1><p>Return to Dictando.</p></div></body></html>')
        elif 'error' in params:
            self.server.auth_error = params.get('error',['unknown'])[0]
            self.send_response(200); self.send_header('Content-Type','text/html'); self.end_headers()
            self.wfile.write(b'<html><body style="font-family:system-ui;display:flex;align-items:center;justify-content:center;height:100vh;margin:0;background:#1a1a1a;color:#e5e5e5"><div style="text-align:center"><h1 style="color:#ef4444">Failed</h1><p>Try again.</p></div></body></html>')
        else:
            self.send_response(404); self.end_headers()


WEB_KEY_MAP = {
    'ControlLeft':'ctrl','ControlRight':'ctrl','ShiftLeft':'shift','ShiftRight':'shift',
    'AltLeft':'alt','AltRight':'alt','MetaLeft':'windows','MetaRight':'windows',
    'Control':'ctrl','Shift':'shift','Alt':'alt','Meta':'windows','Command':'cmd',
    'Space':'space','Enter':'enter','Escape':'esc','Backspace':'backspace',
    'Tab':'tab','Delete':'delete','Insert':'insert','Home':'home','End':'end',
    'PageUp':'page up','PageDown':'page down','CapsLock':'caps lock',
    'ArrowUp':'up','ArrowDown':'down','ArrowLeft':'left','ArrowRight':'right',
}

def parse_web_key(code):
    if not code: return None
    if code in WEB_KEY_MAP: return WEB_KEY_MAP[code]
    if code.startswith('Key') and len(code)==4: return code[3].lower()
    if code.startswith('Digit') and len(code)==6: return code[5]
    if code.startswith('F') and code[1:].isdigit(): return code.lower()
    return code.lower()


class DictationApi:
    def __init__(self):
        self.window = None
        saved = load_local_settings()
        self.settings = {
            "shortcut": saved.get("shortcut", ["Space"]),
            "apiKey": saved.get("apiKey", ""),
            "language": saved.get("language", "Auto-detect"),
            "autoCopy": saved.get("autoCopy", True),
            "autoClear": saved.get("autoClear", False),
        }
        self.is_recording = False
        self.is_running = True
        self.gemini_client = None
        self._keyboard_mod = None
        self._pyaudio_mod = None
        self._pa_instance = None          # persistent PyAudio
        self._sr_mod = None               # pre-loaded speech_recognition
        self._pyperclip_mod = None        # pre-loaded pyperclip

    def _ensure_keyboard(self):
        if self._keyboard_mod is None:
            import keyboard
            self._keyboard_mod = keyboard
            logger.info("keyboard module loaded")
        return self._keyboard_mod

    def _ensure_pyaudio(self):
        if self._pyaudio_mod is None:
            import pyaudio
            self._pyaudio_mod = pyaudio
            logger.info("pyaudio module loaded")
        return self._pyaudio_mod

    def _ensure_pa_instance(self):
        """Create PyAudio instance lazily – only when first dictation fires."""
        if self._pa_instance is None:
            pa_mod = self._ensure_pyaudio()
            self._pa_instance = pa_mod.PyAudio()
            logger.info("PyAudio instance created")
        return self._pa_instance

    def _ensure_sr(self):
        if self._sr_mod is None:
            import speech_recognition as sr
            self._sr_mod = sr
            logger.info("speech_recognition module loaded")
        return self._sr_mod

    def _ensure_pyperclip(self):
        if self._pyperclip_mod is None:
            import pyperclip
            self._pyperclip_mod = pyperclip
            logger.info("pyperclip module loaded")
        return self._pyperclip_mod

    def _ensure_gemini(self):
        if self.gemini_client is None:
            api_key = self.settings.get("apiKey", "")
            if api_key:
                from google import genai
                self.gemini_client = genai.Client(api_key=api_key)
                logger.info("Gemini client ready")
        return self.gemini_client

    # ── settings ──────────────────────────────────────────────────────────
    def sync_settings(self, settings_json):
        try:
            new = json.loads(settings_json)
            self.settings.update(new)
            save_local_settings(self.settings)
            # reset gemini client so it picks up new key
            self.gemini_client = None
            logger.info(f"Settings synced – shortcut={self.settings.get('shortcut')}")
        except Exception as e:
            logger.error(f"sync_settings: {e}")

    # ── auth ──────────────────────────────────────────────────────────────
    def start_google_auth(self):
        threading.Thread(target=self._do_google_auth, daemon=True).start()
        return True

    def _do_google_auth(self):
        try:
            import base64
            cv = secrets.token_urlsafe(64)
            cc = base64.urlsafe_b64encode(hashlib.sha256(cv.encode()).digest()).rstrip(b'=').decode()
            srv = socketserver.TCPServer(("127.0.0.1", 0), OAuthCallbackHandler)
            srv.auth_code = srv.auth_error = None
            port = srv.server_address[1]
            redir = f"http://127.0.0.1:{port}"
            t = threading.Thread(target=srv.handle_request, daemon=True); t.start()
            url = f"{GOOGLE_AUTH_ENDPOINT}?{urllib.parse.urlencode({'client_id':GOOGLE_CLIENT_ID,'redirect_uri':redir,'response_type':'code','scope':'openid email profile','code_challenge':cc,'code_challenge_method':'S256','access_type':'offline'})}"
            webbrowser.open(url)
            t.join(timeout=120); srv.server_close()
            if srv.auth_error:
                self._auth_cb(error=srv.auth_error); return
            if not srv.auth_code:
                self._auth_cb(error="Sign-in timed out."); return
            req = urllib.request.Request(GOOGLE_TOKEN_ENDPOINT,
                data=urllib.parse.urlencode({'code':srv.auth_code,'client_id':GOOGLE_CLIENT_ID,'client_secret':GOOGLE_CLIENT_SECRET,'redirect_uri':redir,'grant_type':'authorization_code','code_verifier':cv}).encode(), method="POST")
            req.add_header("Content-Type","application/x-www-form-urlencoded")
            with urllib.request.urlopen(req) as r:
                tok = json.loads(r.read().decode())
            idt = tok.get("id_token")
            if not idt: self._auth_cb(error="No id_token"); return
            self._auth_cb(id_token=idt)
        except Exception as e:
            logger.error(f"OAuth: {e}"); self._auth_cb(error=str(e))

    def _auth_cb(self, id_token=None, error=None):
        if not self.window: return
        if error:
            e = error.replace("\\","\\\\").replace("'","\\'").replace("`","\\`")
            self.window.evaluate_js(f"window.dispatchEvent(new CustomEvent('googleAuthResult',{{detail:{{error:`{e}`}}}}))")
        else:
            self.window.evaluate_js(f"window.dispatchEvent(new CustomEvent('googleAuthResult',{{detail:{{idToken:'{id_token}'}}}}))")

    # ── hotkey listener ───────────────────────────────────────────────────
    def start_background_listener(self):
        threading.Thread(target=self._delayed_start, daemon=True).start()

    def _delayed_start(self):
        logger.info("Waiting for window to load...")
        time.sleep(3)
        logger.info("Initialising background listener...")
        # Start keyboard listener FIRST – don't let warmup block it
        try:
            kb = self._ensure_keyboard()
        except Exception as e:
            logger.error(f"Cannot load keyboard module: {e}")
            return
        # Pre-load other modules in background (non-blocking)
        threading.Thread(target=self._warmup_modules, daemon=True).start()
        self._listener_loop(kb)

    def _warmup_modules(self):
        """Pre-load modules in background so first dictation is fast."""
        try:
            self._ensure_pyaudio()    # import only, no PyAudio() instance
            self._ensure_sr()
            self._ensure_pyperclip()
            self._ensure_gemini()
            logger.info("All modules pre-loaded")
        except Exception as e:
            logger.warning(f"Warmup partial: {e}")

    def _get_hotkey(self):
        sc = self.settings.get("shortcut", [])
        if not sc: return None
        keys = [parse_web_key(c) for c in sc if parse_web_key(c)]
        return "+".join(keys) if keys else None

    def _listener_loop(self, kb):
        last_hk = ""
        while self.is_running:
            try:
                hk = self._get_hotkey()
                if hk and hk != last_hk:
                    logger.info(f"Listening for: {hk}")
                    last_hk = hk
                if hk and kb.is_pressed(hk):
                    self._do_dictation(kb, hk)
                    while kb.is_pressed(hk):
                        time.sleep(0.05)
                time.sleep(0.05)
            except Exception as e:
                logger.error(f"Listener: {e}")
                time.sleep(1)

    # ── record → transcribe → paste ───────────────────────────────────────
    def _do_dictation(self, kb, hotkey):
        pa_mod = self._ensure_pyaudio()
        pa = self._ensure_pa_instance()

        logger.info("🎙️ Recording…")
        self.is_recording = True
        if self.window:
            try: self.window.evaluate_js("window.dispatchEvent(new CustomEvent('recordingStateChanged',{detail:{isRecording:true}}))")
            except: pass

        stream = pa.open(format=pa_mod.paInt16, channels=CHANNELS, rate=RATE,
                         input=True, frames_per_buffer=CHUNK)
        frames = []
        while kb.is_pressed(hotkey):
            try: frames.append(stream.read(CHUNK, exception_on_overflow=False))
            except: pass

        # Tail buffer: keep recording briefly after key release to capture trailing words
        tail_chunks = int(TAIL_BUFFER_S * RATE / CHUNK)
        for _ in range(tail_chunks):
            try: frames.append(stream.read(CHUNK, exception_on_overflow=False))
            except: break

        stream.stop_stream(); stream.close()
        sw = pa.get_sample_size(pa_mod.paInt16)
        # No pa.terminate() – reuse the persistent instance
        self.is_recording = False

        if self.window:
            try: self.window.evaluate_js("window.dispatchEvent(new CustomEvent('recordingStateChanged',{detail:{isRecording:false}}))")
            except: pass

        if not frames:
            logger.info("No audio"); return

        dur_s = len(frames) * CHUNK / RATE
        logger.info(f"Captured {dur_s:.1f}s – transcribing…")
        threading.Thread(target=self._transcribe_paste, args=(frames, sw, dur_s), daemon=True).start()

    def _get_lang_code(self):
        """Get BCP-47 language code from settings."""
        lang = self.settings.get("language", "Auto-detect")
        if not lang or lang == "Auto-detect":
            return "en-US"
        lang_map = {
            "English":"en-US","Spanish":"es-ES","French":"fr-FR",
            "German":"de-DE","Italian":"it-IT","Portuguese":"pt-BR",
            "Dutch":"nl-NL","Russian":"ru-RU","Chinese":"zh-CN",
            "Japanese":"ja-JP","Korean":"ko-KR","Arabic":"ar-SA",
            "Romanian":"ro-RO","Turkish":"tr-TR","Polish":"pl-PL",
            "Hindi":"hi-IN","Czech":"cs-CZ","Hungarian":"hu-HU",
            "Swedish":"sv-SE","Danish":"da-DK","Finnish":"fi-FI",
            "Greek":"el-GR","Hebrew":"he-IL","Ukrainian":"uk-UA",
            "Vietnamese":"vi-VN","Thai":"th-TH","Indonesian":"id-ID",
            "Malay":"ms-MY","Norwegian":"nb-NO",
        }
        return lang_map.get(lang, lang[:2].lower())

    def _build_wav_bytes(self, frames, sample_width):
        """Build a WAV file entirely in memory – no disk I/O."""
        raw = b''.join(frames)
        buf = io.BytesIO()
        # Write WAV header manually (faster than wave module)
        data_len = len(raw)
        header = struct.pack('<4sI4s4sIHHIIHH4sI',
            b'RIFF', 36 + data_len, b'WAVE',
            b'fmt ', 16, 1, CHANNELS, RATE,
            RATE * CHANNELS * sample_width, CHANNELS * sample_width,
            sample_width * 8,
            b'data', data_len)
        buf.write(header)
        buf.write(raw)
        return buf.getvalue()

    def _transcribe_paste(self, frames, sample_width, dur_s):
        t0 = time.time()
        try:
            # Build WAV in memory – zero disk I/O
            raw_pcm = b''.join(frames)
            wav_bytes = self._build_wav_bytes(frames, sample_width)

            text = None

            # ── Attempt 1: Google Speech API (fast, purpose-built) ──
            try:
                sr = self._ensure_sr()
                audio = sr.AudioData(raw_pcm, RATE, sample_width)
                lang_code = self._get_lang_code()
                recognizer = sr.Recognizer()
                text = recognizer.recognize_google(audio, language=lang_code)
                logger.info(f"✅ Google Speech ({time.time()-t0:.2f}s): {text}")
            except Exception as e:
                logger.warning(f"Google Speech failed ({e}), trying Gemini…")

            # ── Attempt 2: Gemini API (fallback) ──
            if not text:
                client = self._ensure_gemini()
                if client:
                    try:
                        from google.genai import types
                        lang = self.settings.get("language", "Auto-detect")
                        lang_hint = "" if lang == "Auto-detect" else f" The language is {lang}."
                        response = client.models.generate_content(
                            model=GEMINI_MODEL,
                            contents=[
                                types.Part.from_bytes(data=wav_bytes, mime_type="audio/wav"),
                                f"Transcribe this audio exactly as spoken, word for word.{lang_hint} Output ONLY the raw transcription text, nothing else."
                            ]
                        )
                        text = (response.text or "").strip()
                        logger.info(f"✅ Gemini ({time.time()-t0:.2f}s): {text}")
                    except Exception as e:
                        logger.warning(f"Gemini also failed: {e}")
                else:
                    logger.warning("No API key – cannot use Gemini fallback")

            if not text:
                logger.warning("Both engines failed – no transcription")
                return

            # ── Paste into active window ──
            if self.settings.get("autoCopy", True):
                pc = self._ensure_pyperclip()
                pc.copy(text)
                time.sleep(0.05)
                kb = self._ensure_keyboard()
                kb.send('ctrl+v')
                logger.info("Pasted ✓")

            # ── Notify webview ──
            if self.window:
                st = text.replace("\\","\\\\").replace("`","\\`").replace("$","\\$")
                dm = int(dur_s * 1000)
                try: self.window.evaluate_js(f"window.dispatchEvent(new CustomEvent('newDictation',{{detail:{{text:`{st}`,durationMs:{dm}}}}}))")
                except: pass

        except Exception as e:
            logger.error(f"Transcription error: {e}")

    def cleanup(self):
        self.is_running = False
        if self._pa_instance:
            try: self._pa_instance.terminate()
            except: pass


def serve_dir(directory):
    class H(http.server.SimpleHTTPRequestHandler):
        def __init__(self, *a, **kw): super().__init__(*a, directory=directory, **kw)
        def log_message(self, *a): pass
    s = socketserver.TCPServer(("localhost", 0), H)
    threading.Thread(target=s.serve_forever, daemon=True).start()
    return f"http://localhost:{s.server_address[1]}"

def get_base():
    return sys._MEIPASS if getattr(sys, 'frozen', False) else os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def main():
    # Import webview late to ensure it's the first GUI framework loaded
    import webview

    dist = os.path.join(get_base(), "dist")
    url = serve_dir(dist) if os.path.exists(os.path.join(dist, "index.html")) else "http://localhost:5173"
    logger.info(f"URL: {url}")

    api = DictationApi()
    win = webview.create_window('Dictando', url=url, js_api=api,
                                 width=400, height=600, min_size=(350, 500))
    api.window = win
    api.start_background_listener()

    webview.start(debug=False)
    api.cleanup()


if __name__ == '__main__':
    main()
