"""
Dictando – system-wide voice dictation for Windows.
Primary engine: faster-whisper (local, offline, sub-second).
Fallbacks:      Google Web Speech API → Gemini API.
"""
import os, sys, json, logging, threading, time, io, struct
import http.server, socketserver, hashlib, secrets, webbrowser
import urllib.parse, urllib.request

logging.basicConfig(level=logging.INFO, format='%(asctime)s %(levelname)s %(message)s')
logger = logging.getLogger("dictando")

CHUNK = 1024
CHANNELS = 1
RATE = 16000
TAIL_BUFFER_S = 0.15   # extra recording after key release

# faster-whisper config (local inference – no network, sub-second)
FW_MODEL_EN    = "tiny.en"   # English-only, fastest (~75 MB)
FW_MODEL_MULTI = "base"      # Multilingual incl. Romanian (~150 MB)
FW_COMPUTE_TYPE = "int8"     # CPU-optimised quantisation

# Whisper hallucination patterns – phrases it emits on silence/noise
FW_HALLUCINATIONS = {
    "thank you", "thanks for watching", "thanks for watching!",
    "thank you for watching", "thank you for watching!",
    "you", ".", "..", "...", "thanks.", "thank you.",
    "[music]", "[applause]", "[laughter]", "[ music ]",
    "subtitles by", "amara.org",
}

GOOGLE_CLIENT_ID = os.environ.get("GOOGLE_CLIENT_ID", "")
GOOGLE_CLIENT_SECRET = os.environ.get("GOOGLE_CLIENT_SECRET", "")
GOOGLE_AUTH_ENDPOINT = "https://accounts.google.com/o/oauth2/v2/auth"
GOOGLE_TOKEN_ENDPOINT = "https://oauth2.googleapis.com/token"

SETTINGS_FILE = os.path.join(os.path.expanduser("~"), ".dictando_settings.json")
GEMINI_MODEL = "gemini-2.0-flash"

# Tray icon colours (RGBA tuples)
TRAY_IDLE     = (30, 30, 30, 220)
TRAY_RECORDING = (220, 50, 50, 240)
TRAY_PROCESSING = (59, 130, 246, 240)


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


def _is_hallucination(text: str) -> bool:
    """Return True if the text is a known whisper hallucination / empty output."""
    cleaned = text.strip().lower().strip(".,!?")
    return not cleaned or cleaned in FW_HALLUCINATIONS


class DictationApi:
    def __init__(self):
        self.window = None
        saved = load_local_settings()
        self.settings = {
            "shortcut":  saved.get("shortcut",  ["Space"]),
            "apiKey":    saved.get("apiKey",    ""),
            "language":  saved.get("language",  "Auto-detect"),
            "autoCopy":  saved.get("autoCopy",  True),
            "autoClear": saved.get("autoClear", False),
        }
        self.is_recording  = False
        self.is_running    = True
        self._model_loaded = False   # faster-whisper ready?
        self._last_engine  = None   # "faster-whisper" | "google" | "gemini"
        self.gemini_client = None
        self._keyboard_mod  = None
        self._pyaudio_mod   = None
        self._pa_instance   = None   # persistent PyAudio
        self._sr_mod        = None   # pre-loaded speech_recognition (fallback)
        self._pyperclip_mod = None
        self._fw_model      = None   # faster-whisper model instance
        self._fw_model_name = None
        self._tray_icon     = None   # pystray Icon object

    # ── lazy module loaders ───────────────────────────────────────────────

    def _ensure_keyboard(self):
        if self._keyboard_mod is None:
            import keyboard
            self._keyboard_mod = keyboard
        return self._keyboard_mod

    def _ensure_pyaudio(self):
        if self._pyaudio_mod is None:
            import pyaudio
            self._pyaudio_mod = pyaudio
        return self._pyaudio_mod

    def _ensure_pa_instance(self):
        if self._pa_instance is None:
            self._pa_instance = self._ensure_pyaudio().PyAudio()
        return self._pa_instance

    def _ensure_sr(self):
        if self._sr_mod is None:
            import speech_recognition as sr
            self._sr_mod = sr
        return self._sr_mod

    def _ensure_pyperclip(self):
        if self._pyperclip_mod is None:
            import pyperclip
            self._pyperclip_mod = pyperclip
        return self._pyperclip_mod

    def _ensure_gemini(self):
        if self.gemini_client is None:
            api_key = self.settings.get("apiKey", "")
            if api_key:
                from google import genai
                self.gemini_client = genai.Client(api_key=api_key)
        return self.gemini_client

    def _get_fw_lang(self):
        lang = self.settings.get("language", "Auto-detect")
        if not lang or lang == "Auto-detect":
            return None
        fw_map = {
            "English":"en","Spanish":"es","French":"fr","German":"de",
            "Italian":"it","Portuguese":"pt","Dutch":"nl","Russian":"ru",
            "Chinese":"zh","Japanese":"ja","Korean":"ko","Arabic":"ar",
            "Romanian":"ro","Turkish":"tr","Polish":"pl","Hindi":"hi",
            "Czech":"cs","Hungarian":"hu","Swedish":"sv","Danish":"da",
            "Finnish":"fi","Greek":"el","Hebrew":"he","Ukrainian":"uk",
            "Vietnamese":"vi","Thai":"th","Indonesian":"id",
            "Malay":"ms","Norwegian":"no",
        }
        return fw_map.get(lang)

    def _ensure_faster_whisper(self):
        lang = self.settings.get("language", "Auto-detect")
        model_name = FW_MODEL_EN if lang == "English" else FW_MODEL_MULTI
        if self._fw_model is None or self._fw_model_name != model_name:
            logger.info(f"Loading faster-whisper '{model_name}'...")
            from faster_whisper import WhisperModel
            self._fw_model = WhisperModel(model_name, device="cpu", compute_type=FW_COMPUTE_TYPE)
            self._fw_model_name = model_name
            self._model_loaded = True
            logger.info(f"faster-whisper '{model_name}' ready")
        return self._fw_model

    # ── JS-exposed API ────────────────────────────────────────────────────

    def sync_settings(self, settings_json):
        try:
            new = json.loads(settings_json)
            self.settings.update(new)
            save_local_settings(self.settings)
            self.gemini_client = None   # reset so new key is picked up
            logger.info(f"Settings synced – shortcut={self.settings.get('shortcut')}")
        except Exception as e:
            logger.error(f"sync_settings: {e}")

    def get_status(self):
        """Return a JSON string describing current app state for the web UI."""
        return json.dumps({
            "modelLoaded": self._model_loaded,
            "modelName":   self._fw_model_name,
            "lastEngine":  self._last_engine,
            "hotkey":      self._get_hotkey(),
            "isRecording": self.is_recording,
        })

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
        try:
            kb = self._ensure_keyboard()
        except Exception as e:
            logger.error(f"Cannot load keyboard module: {e}")
            return
        threading.Thread(target=self._warmup_modules, daemon=True).start()
        self._listener_loop(kb)

    def _warmup_modules(self):
        """Pre-load modules so first dictation fires immediately."""
        try:
            self._ensure_pyaudio()
            self._ensure_pyperclip()
            self._ensure_faster_whisper()   # downloads + compiles model once
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

        logger.info("Recording...")
        self.is_recording = True
        self._set_tray_state("recording")
        self._dispatch("recordingStateChanged", '{"isRecording":true}')

        stream = pa.open(format=pa_mod.paInt16, channels=CHANNELS, rate=RATE,
                         input=True, frames_per_buffer=CHUNK)
        frames = []
        while kb.is_pressed(hotkey):
            try: frames.append(stream.read(CHUNK, exception_on_overflow=False))
            except: pass

        # Tail buffer: keep recording briefly after key release
        tail_chunks = int(TAIL_BUFFER_S * RATE / CHUNK)
        for _ in range(tail_chunks):
            try: frames.append(stream.read(CHUNK, exception_on_overflow=False))
            except: break

        stream.stop_stream(); stream.close()
        sw = pa.get_sample_size(pa_mod.paInt16)
        self.is_recording = False
        self._set_tray_state("processing")
        self._dispatch("recordingStateChanged", '{"isRecording":false}')

        if not frames:
            logger.info("No audio captured")
            self._set_tray_state("idle")
            return

        dur_s = len(frames) * CHUNK / RATE
        logger.info(f"Captured {dur_s:.1f}s — transcribing...")
        threading.Thread(target=self._transcribe_paste, args=(frames, sw, dur_s), daemon=True).start()

    def _get_lang_code(self):
        lang = self.settings.get("language", "Auto-detect")
        if not lang or lang == "Auto-detect": return "en-US"
        lang_map = {
            "English":"en-US","Spanish":"es-ES","French":"fr-FR","German":"de-DE",
            "Italian":"it-IT","Portuguese":"pt-BR","Dutch":"nl-NL","Russian":"ru-RU",
            "Chinese":"zh-CN","Japanese":"ja-JP","Korean":"ko-KR","Arabic":"ar-SA",
            "Romanian":"ro-RO","Turkish":"tr-TR","Polish":"pl-PL","Hindi":"hi-IN",
            "Czech":"cs-CZ","Hungarian":"hu-HU","Swedish":"sv-SE","Danish":"da-DK",
            "Finnish":"fi-FI","Greek":"el-GR","Hebrew":"he-IL","Ukrainian":"uk-UA",
            "Vietnamese":"vi-VN","Thai":"th-TH","Indonesian":"id-ID",
            "Malay":"ms-MY","Norwegian":"nb-NO",
        }
        return lang_map.get(lang, lang[:2].lower())

    def _build_wav_bytes(self, frames, sample_width):
        """Build a WAV file entirely in memory – no disk I/O."""
        raw = b''.join(frames)
        data_len = len(raw)
        header = struct.pack('<4sI4s4sIHHIIHH4sI',
            b'RIFF', 36 + data_len, b'WAVE',
            b'fmt ', 16, 1, CHANNELS, RATE,
            RATE * CHANNELS * sample_width, CHANNELS * sample_width,
            sample_width * 8,
            b'data', data_len)
        return header + raw

    def _transcribe_paste(self, frames, sample_width, dur_s):
        t0 = time.time()
        try:
            raw_pcm = b''.join(frames)
            text = None

            # ── Primary: faster-whisper (local, no network, sub-second) ──
            try:
                import numpy as np
                model = self._ensure_faster_whisper()
                fw_lang = self._get_fw_lang()
                audio_np = np.frombuffer(raw_pcm, dtype=np.int16).astype(np.float32) / 32768.0
                segments, _info = model.transcribe(
                    audio_np,
                    language=fw_lang,
                    beam_size=1,
                    condition_on_previous_text=False,
                    vad_filter=True,
                    vad_parameters={"min_silence_duration_ms": 100},
                )
                raw_text = " ".join(seg.text.strip() for seg in segments).strip()
                if raw_text and not _is_hallucination(raw_text):
                    text = raw_text
                    self._last_engine = "faster-whisper"
                    logger.info(f"faster-whisper ({time.time()-t0:.3f}s): {text!r}")
                elif raw_text:
                    logger.info(f"Hallucination suppressed: {raw_text!r}")
            except Exception as e:
                logger.warning(f"faster-whisper failed ({e}), trying Google Speech...")

            # ── Fallback 1: Google Web Speech API ────────────────────────
            if not text:
                try:
                    sr = self._ensure_sr()
                    audio = sr.AudioData(raw_pcm, RATE, sample_width)
                    lang_code = self._get_lang_code()
                    recognizer = sr.Recognizer()
                    text = recognizer.recognize_google(audio, language=lang_code)
                    if text:
                        self._last_engine = "google"
                        logger.info(f"Google Speech ({time.time()-t0:.2f}s): {text!r}")
                except Exception as e:
                    logger.warning(f"Google Speech failed ({e}), trying Gemini...")

            # ── Fallback 2: Gemini API ────────────────────────────────────
            if not text:
                client = self._ensure_gemini()
                if client:
                    try:
                        from google.genai import types
                        wav_bytes = self._build_wav_bytes(frames, sample_width)
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
                        if text:
                            self._last_engine = "gemini"
                            logger.info(f"Gemini ({time.time()-t0:.2f}s): {text!r}")
                    except Exception as e:
                        logger.warning(f"Gemini also failed: {e}")

            self._set_tray_state("idle")

            if not text:
                logger.warning("All engines failed – no transcription")
                return

            # ── Paste into active window ──────────────────────────────────
            if self.settings.get("autoCopy", True):
                pc = self._ensure_pyperclip()
                pc.copy(text)
                time.sleep(0.05)
                kb = self._ensure_keyboard()
                kb.send('ctrl+v')
                logger.info("Pasted")

            # ── Notify webview ────────────────────────────────────────────
            if self.window:
                st = text.replace("\\","\\\\").replace("`","\\`").replace("$","\\$")
                dm = int(dur_s * 1000)
                eng = self._last_engine or "unknown"
                try:
                    self.window.evaluate_js(
                        f"window.dispatchEvent(new CustomEvent('newDictation',"
                        f"{{detail:{{text:`{st}`,durationMs:{dm},engine:`{eng}`}}}}))"
                    )
                except: pass

        except Exception as e:
            logger.error(f"Transcription error: {e}")
            self._set_tray_state("idle")

    # ── status helpers ────────────────────────────────────────────────────

    def _notify_status(self):
        """Push current status to the webview."""
        if not self.window: return
        try:
            status = self.get_status()
            safe = status.replace("`", "\\`")
            self.window.evaluate_js(
                f"window.dispatchEvent(new CustomEvent('dictandoStatus',{{detail:JSON.parse(`{safe}`)}}))"
            )
        except: pass

    def _dispatch(self, event_name, detail_json):
        if not self.window: return
        try:
            self.window.evaluate_js(
                f"window.dispatchEvent(new CustomEvent('{event_name}',{{detail:{detail_json}}}))"
            )
        except: pass

    # ── system tray ───────────────────────────────────────────────────────

    def _set_tray_state(self, state: str):
        """Update tray icon colour/tooltip to reflect current state."""
        if not self._tray_icon: return
        try:
            import pystray
            from PIL import Image, ImageDraw
            colours = {
                "idle":       TRAY_IDLE,
                "recording":  TRAY_RECORDING,
                "processing": TRAY_PROCESSING,
            }
            colour = colours.get(state, TRAY_IDLE)
            img = Image.new("RGBA", (64, 64), (0, 0, 0, 0))
            draw = ImageDraw.Draw(img)
            draw.ellipse([4, 4, 60, 60], fill=colour)
            if state == "recording":
                # Draw mic shape
                draw.rectangle([24, 16, 40, 40], fill=(255, 255, 255, 200))
                draw.ellipse([20, 12, 44, 36], fill=(255, 255, 255, 200))
            self._tray_icon.icon = img
            tooltips = {
                "idle":       "Dictando — Ready",
                "recording":  "Dictando — Recording...",
                "processing": "Dictando — Transcribing...",
            }
            self._tray_icon.title = tooltips.get(state, "Dictando")
        except Exception as e:
            logger.debug(f"Tray update: {e}")

    def start_tray(self, win):
        """Launch system tray icon (call after window is created)."""
        try:
            import pystray
            from PIL import Image, ImageDraw
            img = Image.new("RGBA", (64, 64), (0, 0, 0, 0))
            draw = ImageDraw.Draw(img)
            draw.ellipse([4, 4, 60, 60], fill=TRAY_IDLE)

            def show_window(icon, item):
                win.show()

            def quit_app(icon, item):
                self.is_running = False
                icon.stop()
                win.destroy()

            menu = pystray.Menu(
                pystray.MenuItem("Show Dictando", show_window, default=True),
                pystray.Menu.SEPARATOR,
                pystray.MenuItem("Quit", quit_app),
            )
            self._tray_icon = pystray.Icon("Dictando", img, "Dictando — Ready", menu)
            threading.Thread(target=self._tray_icon.run, daemon=True).start()
            logger.info("System tray started")
        except Exception as e:
            logger.warning(f"Tray not available: {e}")

    # ── cleanup ───────────────────────────────────────────────────────────

    def cleanup(self):
        self.is_running = False
        if self._pa_instance:
            try: self._pa_instance.terminate()
            except: pass
        if self._tray_icon:
            try: self._tray_icon.stop()
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
    import webview

    dist = os.path.join(get_base(), "dist")
    url = serve_dir(dist) if os.path.exists(os.path.join(dist, "index.html")) else "http://localhost:5173"
    logger.info(f"URL: {url}")

    api = DictationApi()
    win = webview.create_window(
        'Dictando', url=url, js_api=api,
        width=400, height=640, min_size=(350, 520),
    )
    api.window = win
    api.start_background_listener()
    # api.start_tray(win)  # disabled: pystray's hidden HWND conflicts with pywebview

    webview.start(debug=False)
    api.cleanup()


if __name__ == '__main__':
    main()
