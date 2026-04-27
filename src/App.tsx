import React, { useState, useEffect, useRef, Component, ErrorInfo, ReactNode } from 'react';
import { auth, db, googleProvider } from './firebase';
import { signOut, onAuthStateChanged, User } from 'firebase/auth';
import { collection, doc, setDoc, getDoc, addDoc, query, orderBy, limit, onSnapshot, serverTimestamp } from 'firebase/firestore';
import { GoogleGenAI } from '@google/genai';
import { Mic, Settings, History, LogOut, Play, Square, Loader2, Copy, Check } from 'lucide-react';
import { Toaster, toast } from 'sonner';
import { motion, AnimatePresence } from 'motion/react';

enum OperationType {
  CREATE = 'create',
  UPDATE = 'update',
  DELETE = 'delete',
  LIST = 'list',
  GET = 'get',
  WRITE = 'write',
}

interface FirestoreErrorInfo {
  error: string;
  operationType: OperationType;
  path: string | null;
  authInfo: {
    userId: string | undefined;
    email: string | null | undefined;
    emailVerified: boolean | undefined;
    isAnonymous: boolean | undefined;
    tenantId: string | null | undefined;
    providerInfo: {
      providerId: string;
      displayName: string | null;
      email: string | null;
      photoUrl: string | null;
    }[];
  }
}

function handleFirestoreError(error: unknown, operationType: OperationType, path: string | null) {
  const errInfo: FirestoreErrorInfo = {
    error: error instanceof Error ? error.message : String(error),
    authInfo: {
      userId: auth.currentUser?.uid,
      email: auth.currentUser?.email,
      emailVerified: auth.currentUser?.emailVerified,
      isAnonymous: auth.currentUser?.isAnonymous,
      tenantId: auth.currentUser?.tenantId,
      providerInfo: auth.currentUser?.providerData.map(provider => ({
        providerId: provider.providerId,
        displayName: provider.displayName,
        email: provider.email,
        photoUrl: provider.photoURL
      })) || []
    },
    operationType,
    path
  }
  console.error('Firestore Error: ', JSON.stringify(errInfo));
  throw new Error(JSON.stringify(errInfo));
}

interface ErrorBoundaryProps {
  children: ReactNode;
}

interface ErrorBoundaryState {
  hasError: boolean;
  error: Error | null;
}

class ErrorBoundary extends Component<ErrorBoundaryProps, ErrorBoundaryState> {
  constructor(props: ErrorBoundaryProps) {
    super(props);
    this.state = { hasError: false, error: null };
  }

  static getDerivedStateFromError(error: Error) {
    return { hasError: true, error };
  }

  componentDidCatch(error: Error, errorInfo: ErrorInfo) {
    console.error("Uncaught error:", error, errorInfo);
  }

  render() {
    if (this.state.hasError) {
      let message = "Something went wrong.";
      try {
        const parsed = JSON.parse(this.state.error?.message || "");
        if (parsed.error && parsed.error.includes("insufficient permissions")) {
          message = "You don't have permission to perform this action. Please check your settings or contact support.";
        }
      } catch (e) {
        // Not a JSON error
      }

      return (
        <div className="min-h-screen bg-zinc-950 flex flex-col items-center justify-center p-6 text-center">
          <div className="w-16 h-16 bg-red-500/10 rounded-full flex items-center justify-center mb-6">
            <Settings className="w-8 h-8 text-red-500" />
          </div>
          <h1 className="text-xl font-bold text-zinc-100 mb-2">Application Error</h1>
          <p className="text-zinc-400 max-w-xs mb-8">{message}</p>
          <button
            onClick={() => window.location.reload()}
            className="px-6 py-2 bg-zinc-800 hover:bg-zinc-700 text-zinc-100 rounded-lg transition-colors"
          >
            Reload App
          </button>
        </div>
      );
    }

    return this.props.children;
  }
}

// Main App Component
export default function App() {
  return (
    <ErrorBoundary>
      <Toaster theme="dark" position="bottom-center" />
      <AppContent />
    </ErrorBoundary>
  );
}

function AppContent() {
  const [user, setUser] = useState<User | null>(null);
  const [isAuthReady, setIsAuthReady] = useState(false);
  const [activeTab, setActiveTab] = useState<'record' | 'history' | 'settings'>('record');

  useEffect(() => {
    const unsubscribe = onAuthStateChanged(auth, async (currentUser) => {
      setUser(currentUser);
      setIsAuthReady(true);

      if (currentUser) {
        // Ensure user document exists
        const userRef = doc(db, 'users', currentUser.uid);
        try {
          const userSnap = await getDoc(userRef);
          if (!userSnap.exists()) {
            await setDoc(userRef, {
              email: currentUser.email,
              displayName: currentUser.displayName || '',
              photoURL: currentUser.photoURL || '',
              shortcut: ['Space'],
              createdAt: serverTimestamp()
            });
          }
        } catch (error) {
          handleFirestoreError(error, OperationType.WRITE, `users/${currentUser.uid}`);
        }
      }
    });
    return () => unsubscribe();
  }, []);

  // Listen for Python background dictations
  useEffect(() => {
    if (!user) return;

    const handleNewDictation = async (e: Event) => {
      const customEvent = e as CustomEvent<{ text: string, durationMs: number }>;
      const { text, durationMs } = customEvent.detail;

      if (!text) return;

      const path = `users/${user.uid}/dictations`;
      try {
        await addDoc(collection(db, path), {
          text,
          timestamp: serverTimestamp(),
          durationMs
        });
        toast.success("Dictation saved from background");
      } catch (error) {
        handleFirestoreError(error, OperationType.CREATE, path);
      }
    };

    window.addEventListener('newDictation', handleNewDictation);
    return () => window.removeEventListener('newDictation', handleNewDictation);
  }, [user]);

  // Listen for Python background recording state
  useEffect(() => {
    const handleRecordingState = (e: Event) => {
      const ce = e as CustomEvent<{ isRecording: boolean }>;
      if (ce.detail.isRecording) {
        toast.info("Recording started in background...");
      } else {
        toast.success("Recording finished, processing...");
      }
    };
    window.addEventListener('recordingStateChanged', handleRecordingState);
    return () => window.removeEventListener('recordingStateChanged', handleRecordingState);
  }, []);

  if (!isAuthReady) {
    return (
      <div className="min-h-screen bg-zinc-950 flex items-center justify-center">
        <Loader2 className="w-8 h-8 text-zinc-500 animate-spin" />
      </div>
    );
  }

  if (!user) {
    return <LoginScreen />;
  }

  return (
    <div className="min-h-screen bg-zinc-950 text-zinc-100 font-sans flex items-center justify-center p-4">

      {/* Simulated macOS floating window */}
      <div className="w-full max-w-sm bg-zinc-900/80 backdrop-blur-xl border border-zinc-800 rounded-2xl shadow-2xl overflow-hidden flex flex-col h-[600px]">

        {/* Header / Drag Handle */}
        <div className="h-12 border-b border-zinc-800/50 flex items-center justify-between px-4 bg-zinc-900/50">
          <div className="flex items-center gap-2">
            <div className="w-3 h-3 rounded-full bg-red-500/20 border border-red-500/50" />
            <div className="w-3 h-3 rounded-full bg-yellow-500/20 border border-yellow-500/50" />
            <div className="w-3 h-3 rounded-full bg-green-500/20 border border-green-500/50" />
          </div>
          <div className="text-xs font-medium text-zinc-400 tracking-wide">DICTANDO</div>
          <div className="w-10" /> {/* Spacer for balance */}
        </div>

        {/* Main Content Area */}
        <div className="flex-1 overflow-y-auto relative">
          <AnimatePresence mode="wait">
            {activeTab === 'record' && <RecordView key="record" user={user} />}
            {activeTab === 'history' && <HistoryView key="history" user={user} />}
            {activeTab === 'settings' && <SettingsView key="settings" user={user} />}
          </AnimatePresence>
        </div>

        {/* Bottom Navigation */}
        <div className="h-16 border-t border-zinc-800/50 bg-zinc-900/80 flex items-center justify-around px-2">
          <NavButton
            icon={<Mic size={20} />}
            label="Dictate"
            isActive={activeTab === 'record'}
            onClick={() => setActiveTab('record')}
          />
          <NavButton
            icon={<History size={20} />}
            label="History"
            isActive={activeTab === 'history'}
            onClick={() => setActiveTab('history')}
          />
          <NavButton
            icon={<Settings size={20} />}
            label="Settings"
            isActive={activeTab === 'settings'}
            onClick={() => setActiveTab('settings')}
          />
        </div>
      </div>
    </div>
  );
}

function NavButton({ icon, label, isActive, onClick }: { icon: React.ReactNode, label: string, isActive: boolean, onClick: () => void }) {
  return (
    <button
      onClick={onClick}
      className={`flex flex-col items-center justify-center w-16 h-12 rounded-xl transition-all ${isActive ? 'text-blue-400 bg-blue-500/10' : 'text-zinc-500 hover:text-zinc-300 hover:bg-zinc-800/50'
        }`}
    >
      {icon}
      <span className="text-[10px] font-medium mt-1">{label}</span>
    </button>
  );
}

function LoginScreen() {
  const [isLoading, setIsLoading] = useState(false);

  useEffect(() => {
    // Listen for auth result from the Python bridge (system browser flow)
    const handleAuthResult = async (e: any) => {
      const detail = e.detail;
      if (detail.error) {
        toast.error(`Sign-in failed: ${detail.error}`);
        setIsLoading(false);
        return;
      }

      try {
        const { GoogleAuthProvider, signInWithCredential } = await import('firebase/auth');
        const credential = GoogleAuthProvider.credential(detail.idToken);
        await signInWithCredential(auth, credential);
      } catch (err: any) {
        console.error("Firebase credential sign-in failed", err);
        toast.error(`Sign-in failed: ${err?.message || 'Unknown error'}`);
      } finally {
        setIsLoading(false);
      }
    };

    window.addEventListener('googleAuthResult', handleAuthResult);
    return () => window.removeEventListener('googleAuthResult', handleAuthResult);
  }, []);

  const handleLogin = async () => {
    setIsLoading(true);

    // Check pywebview at click time, not mount time (may load late)
    const pyApi = (window as any).pywebview?.api;

    if (pyApi) {
      try {
        pyApi.start_google_auth();
      } catch (err: any) {
        toast.error(`Sign-in failed: ${err?.message || 'Unknown error'}`);
        setIsLoading(false);
      }
    } else {
      try {
        const { signInWithPopup } = await import('firebase/auth');
        await signInWithPopup(auth, googleProvider);
      } catch (error: any) {
        console.error("Login failed", error);
        if (error?.code !== 'auth/popup-closed-by-user') {
          toast.error(`Sign-in failed: ${error?.message || 'Unknown error'}`);
        }
      } finally {
        setIsLoading(false);
      }
    }
  };

  return (
    <div className="min-h-screen bg-zinc-950 flex flex-col items-center justify-center p-4">
      <div className="w-full max-w-sm bg-zinc-900 border border-zinc-800 rounded-3xl p-8 text-center shadow-2xl">
        <div className="w-16 h-16 bg-blue-500/20 text-blue-400 rounded-2xl flex items-center justify-center mx-auto mb-6">
          <Mic size={32} />
        </div>
        <h1 className="text-2xl font-semibold text-zinc-100 mb-2">Dictando</h1>
        <p className="text-zinc-400 text-sm mb-8">Instant, multilingual speech-to-text.</p>

        <button
          onClick={handleLogin}
          disabled={isLoading}
          className="w-full bg-zinc-100 text-zinc-900 hover:bg-white font-medium py-3 px-4 rounded-xl transition-colors flex items-center justify-center gap-2 disabled:opacity-60 disabled:cursor-not-allowed"
        >
          {isLoading ? (
            <Loader2 className="w-5 h-5 animate-spin" />
          ) : (
            <svg className="w-5 h-5" viewBox="0 0 24 24">
              <path fill="currentColor" d="M22.56 12.25c0-.78-.07-1.53-.2-2.25H12v4.26h5.92c-.26 1.37-1.04 2.53-2.21 3.31v2.77h3.57c2.08-1.92 3.28-4.74 3.28-8.09z" />
              <path fill="#34A853" d="M12 23c2.97 0 5.46-.98 7.28-2.66l-3.57-2.77c-.98.66-2.23 1.06-3.71 1.06-2.86 0-5.29-1.93-6.16-4.53H2.18v2.84C3.99 20.53 7.7 23 12 23z" />
              <path fill="#FBBC05" d="M5.84 14.09c-.22-.66-.35-1.36-.35-2.09s.13-1.43.35-2.09V7.07H2.18C1.43 8.55 1 10.22 1 12s.43 3.45 1.18 4.93l2.85-2.22.81-.62z" />
              <path fill="#EA4335" d="M12 5.38c1.62 0 3.06.56 4.21 1.64l3.15-3.15C17.45 2.09 14.97 1 12 1 7.7 1 3.99 3.47 2.18 7.07l3.66 2.84c.87-2.6 3.3-4.53 6.16-4.53z" />
            </svg>
          )}
          {isLoading ? 'Signing in...' : 'Continue with Google'}
        </button>
      </div>
    </div>
  );
}

function RecordView({ user }: { user: User; key?: string }) {
  const [isRecording, setIsRecording] = useState(false);
  const [isProcessing, setIsProcessing] = useState(false);
  const [transcription, setTranscription] = useState<string | null>(null);
  const [settings, setSettings] = useState({
    shortcut: ['Space'],
    language: 'Auto-detect',
    autoCopy: true,
    autoClear: false,
    microphoneId: 'default',
    apiKey: ''
  });
  const mediaRecorderRef = useRef<MediaRecorder | null>(null);
  const audioChunksRef = useRef<Blob[]>([]);
  const startTimeRef = useRef<number>(0);
  const pressedKeys = useRef<Set<string>>(new Set());
  const apiKeyRef = useRef<string>(localStorage.getItem('dictando_apiKey') || '');

  // Load settings from Firestore
  useEffect(() => {
    const unsubscribe = onSnapshot(doc(db, 'users', user.uid), (docSnap) => {
      if (docSnap.exists()) {
        const data = docSnap.data();
        const key = data.apiKey || localStorage.getItem('dictando_apiKey') || '';
        apiKeyRef.current = key;
        setSettings({
          shortcut: Array.isArray(data.shortcut) ? data.shortcut : (data.shortcut ? [data.shortcut] : ['Space']),
          language: data.language || 'Auto-detect',
          autoCopy: data.autoCopy !== undefined ? data.autoCopy : true,
          autoClear: data.autoClear !== undefined ? data.autoClear : false,
          microphoneId: data.microphoneId || 'default',
          apiKey: key
        });
      }
    }, (error) => {
      console.error('Firestore settings listener error:', error);
      // Fallback to localStorage
      const key = localStorage.getItem('dictando_apiKey') || '';
      if (key) {
        apiKeyRef.current = key;
        setSettings(prev => ({ ...prev, apiKey: key }));
      }
    });
    return () => unsubscribe();
  }, [user.uid]);

  // Simulate global shortcut with spacebar when this view is focused
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      // Don't trigger if typing in an input (though we don't have any yet)
      if (e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement) return;

      pressedKeys.current.add(e.code);

      // Check if all shortcut keys are pressed
      const allPressed = Array.isArray(settings.shortcut) && settings.shortcut.every(key => pressedKeys.current.has(key));

      if (allPressed && !isRecording && !isProcessing) {
        e.preventDefault();
        startRecording();
      }
    };

    const handleKeyUp = (e: KeyboardEvent) => {
      pressedKeys.current.delete(e.code);

      // If any of the shortcut keys are released, stop recording
      const shortcutKeyReleased = Array.isArray(settings.shortcut) && settings.shortcut.includes(e.code);

      if (shortcutKeyReleased && isRecording) {
        e.preventDefault();
        stopRecording();
      }
    };

    // Handle window blur to clear keys (prevents stuck keys)
    const handleBlur = () => {
      pressedKeys.current.clear();
      if (isRecording) stopRecording();
    };

    window.addEventListener('keydown', handleKeyDown);
    window.addEventListener('keyup', handleKeyUp);
    window.addEventListener('blur', handleBlur);
    return () => {
      window.removeEventListener('keydown', handleKeyDown);
      window.removeEventListener('keyup', handleKeyUp);
      window.removeEventListener('blur', handleBlur);
    };
  }, [isRecording, isProcessing, settings.shortcut]);

  const startRecording = async () => {
    try {
      const audioConstraints: boolean | MediaTrackConstraints =
        settings.microphoneId && settings.microphoneId !== 'default'
          ? { deviceId: { exact: settings.microphoneId } }
          : true;

      const stream = await navigator.mediaDevices.getUserMedia({ audio: audioConstraints });

      // Prefer audio/webm if available, fallback to default
      const options = MediaRecorder.isTypeSupported('audio/webm') ? { mimeType: 'audio/webm' } : undefined;
      const mediaRecorder = new MediaRecorder(stream, options);

      mediaRecorderRef.current = mediaRecorder;
      audioChunksRef.current = [];
      startTimeRef.current = Date.now();

      mediaRecorder.ondataavailable = (e) => {
        if (e.data.size > 0) {
          audioChunksRef.current.push(e.data);
        }
      };

      mediaRecorder.onstop = async () => {
        const durationMs = Date.now() - startTimeRef.current;

        if (audioChunksRef.current.length === 0) {
          console.error("No audio chunks collected");
          toast.error("Recording failed: No audio detected");
          setIsProcessing(false);
          return;
        }

        const audioBlob = new Blob(audioChunksRef.current, { type: mediaRecorder.mimeType || 'audio/webm' });

        // Stop all tracks to release microphone
        stream.getTracks().forEach(track => track.stop());

        await processAudio(audioBlob, durationMs);
      };

      mediaRecorder.start();
      setIsRecording(true);
      setTranscription(null);
    } catch (err) {
      console.error("Error accessing microphone:", err);
      toast.error("Microphone access denied");
    }
  };

  const stopRecording = () => {
    if (mediaRecorderRef.current && isRecording) {
      mediaRecorderRef.current.stop();
      setIsRecording(false);
      setIsProcessing(true);
    }
  };

  const processAudio = async (audioBlob: Blob, durationMs: number) => {
    try {
      // Convert Blob to Base64
      const reader = new FileReader();
      reader.readAsDataURL(audioBlob);
      reader.onloadend = async () => {
        const base64data = reader.result as string;
        // Remove the data URL prefix (e.g., "data:audio/webm;base64,")
        const base64String = base64data.split(',')[1];

        try {
          const apiKey = apiKeyRef.current || settings.apiKey || localStorage.getItem('dictando_apiKey') || (import.meta as any).env.VITE_GEMINI_API_KEY || (process.env as any).GEMINI_API_KEY;
          console.log("API Key present:", !!apiKey);
          if (!apiKey) {
            console.error("Gemini API Key is missing");
            toast.error("API Key missing. Please check settings.");
            setIsProcessing(false);
            return;
          }

          console.log("Audio MIME type:", audioBlob.type);
          console.log("Audio size:", audioBlob.size);

          if (audioBlob.size < 100) {
            toast.error("Recording too short");
            setIsProcessing(false);
            return;
          }

          // Create a fresh instance to ensure we use the latest key
          const genAI = new GoogleGenAI({ apiKey });

          // Use gemini-3.1-pro-preview as it's the standard multimodal model
          const response = await genAI.models.generateContent({
            model: 'gemini-3.1-pro-preview',
            contents: {
              parts: [
                {
                  inlineData: {
                    mimeType: audioBlob.type.split(';')[0] || 'audio/webm',
                    data: base64String
                  }
                },
                {
                  text: `Transcribe the following audio exactly as spoken. ${settings.language !== 'Auto-detect' ? `The language is ${settings.language}. ` : 'Detect the language automatically. '}Do not add any extra text, commentary, or markdown formatting. Just the raw text.`
                }
              ]
            }
          });

          const text = response.text?.trim() || "";
          setTranscription(text);

          if (text) {
            // Save to Firestore
            const path = `users/${user.uid}/dictations`;
            try {
              await addDoc(collection(db, path), {
                text,
                timestamp: serverTimestamp(),
                durationMs
              });
            } catch (error) {
              handleFirestoreError(error, OperationType.CREATE, path);
            }

            // Auto-copy to clipboard
            if (settings.autoCopy) {
              try {
                await navigator.clipboard.writeText(text);
                toast.success("Copied to clipboard!");
              } catch (err) {
                console.warn("Clipboard write failed:", err);
                toast.error("Transcription ready (couldn't auto-copy).");
              }
            }

            if (settings.autoClear) {
              setTimeout(() => setTranscription(null), 3000);
            }
          }
        } catch (apiError: any) {
          console.error("Gemini API Error:", apiError);
          const message = apiError?.message || "";
          if (message.includes("API key not valid")) {
            toast.error("Invalid API Key. Please check settings.");
          } else if (message.includes("quota")) {
            toast.error("API quota exceeded. Try again later.");
          } else {
            toast.error("Transcription failed. Check console for details.");
          }
        } finally {
          setIsProcessing(false);
        }
      };
    } catch (err) {
      console.error("Processing error:", err);
      setIsProcessing(false);
      toast.error("Failed to process audio");
    }
  };

  return (
    <motion.div
      initial={{ opacity: 0, y: 10 }}
      animate={{ opacity: 1, y: 0 }}
      exit={{ opacity: 0, y: -10 }}
      className="absolute inset-0 flex flex-col items-center justify-center p-6"
    >
      <div className="flex-1 w-full flex flex-col items-center justify-center">

        {/* Status Text */}
        <div className="h-8 mb-8 flex items-center justify-center">
          {isRecording ? (
            <span className="text-red-400 font-medium animate-pulse flex items-center gap-2">
              <span className="w-2 h-2 rounded-full bg-red-500" /> Recording...
            </span>
          ) : isProcessing ? (
            <span className="text-blue-400 font-medium flex items-center gap-2">
              <Loader2 className="w-4 h-4 animate-spin" /> Transcribing...
            </span>
          ) : (
            <span className="text-zinc-500 text-sm flex items-center gap-2">
              Hold
              <span className="flex gap-1">
                {Array.isArray(settings.shortcut) && settings.shortcut.map(key => (
                  <kbd key={key} className="px-1.5 py-0.5 bg-zinc-800 border border-zinc-700 rounded text-[10px] font-mono">
                    {key.replace('Key', '').replace('Digit', '').replace('Left', '').replace('Right', '').replace('Space', 'Space')}
                  </kbd>
                ))}
              </span>
              to dictate
            </span>
          )}
        </div>

        {/* Big Record Button */}
        <button
          onMouseDown={startRecording}
          onMouseUp={stopRecording}
          onMouseLeave={stopRecording}
          onTouchStart={startRecording}
          onTouchEnd={stopRecording}
          disabled={isProcessing}
          className={`relative w-32 h-32 rounded-full flex items-center justify-center transition-all duration-300 ${isRecording
            ? 'bg-red-500/20 text-red-500 scale-110 shadow-[0_0_40px_rgba(239,68,68,0.3)]'
            : isProcessing
              ? 'bg-zinc-800 text-zinc-600 cursor-not-allowed'
              : 'bg-zinc-800 text-zinc-300 hover:bg-zinc-700 hover:scale-105 shadow-xl'
            }`}
        >
          {isRecording ? (
            <Square size={40} className="fill-current" />
          ) : (
            <Mic size={48} />
          )}

          {/* Ripple effect when recording */}
          {isRecording && (
            <>
              <div className="absolute inset-0 rounded-full border-2 border-red-500/50 animate-ping" style={{ animationDuration: '1.5s' }} />
              <div className="absolute inset-[-20px] rounded-full border border-red-500/30 animate-ping" style={{ animationDuration: '2s', animationDelay: '0.5s' }} />
            </>
          )}
        </button>

      </div>

      {/* Transcription Result */}
      <div className="h-32 w-full mt-8">
        <AnimatePresence>
          {transcription && (
            <motion.div
              initial={{ opacity: 0, y: 10 }}
              animate={{ opacity: 1, y: 0 }}
              className="w-full bg-zinc-800/50 border border-zinc-700/50 rounded-xl p-4 relative group"
            >
              <p className="text-zinc-200 text-sm leading-relaxed line-clamp-3">{transcription}</p>
              <button
                onClick={async () => {
                  try {
                    await navigator.clipboard.writeText(transcription);
                    toast.success("Copied!");
                  } catch (err) {
                    console.warn("Clipboard write failed:", err);
                    toast.error("Failed to copy to clipboard.");
                  }
                }}
                className="absolute top-2 right-2 p-1.5 bg-zinc-700/80 text-zinc-300 rounded-md opacity-0 group-hover:opacity-100 transition-opacity hover:bg-zinc-600 hover:text-white"
              >
                <Copy size={14} />
              </button>
            </motion.div>
          )}
        </AnimatePresence>
      </div>
    </motion.div>
  );
}

function HistoryView({ user }: { user: User; key?: string }) {
  const [history, setHistory] = useState<any[]>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    const path = `users/${user.uid}/dictations`;
    const q = query(
      collection(db, path),
      orderBy('timestamp', 'desc'),
      limit(20)
    );

    const unsubscribe = onSnapshot(q, (snapshot) => {
      const docs = snapshot.docs.map(doc => ({
        id: doc.id,
        ...doc.data()
      }));
      setHistory(docs);
      setLoading(false);
    }, (error) => {
      handleFirestoreError(error, OperationType.LIST, path);
    });

    return () => unsubscribe();
  }, [user.uid]);

  return (
    <motion.div
      initial={{ opacity: 0, x: 20 }}
      animate={{ opacity: 1, x: 0 }}
      exit={{ opacity: 0, x: -20 }}
      className="absolute inset-0 p-4 overflow-y-auto"
    >
      <h2 className="text-lg font-medium text-zinc-100 mb-4 px-2">Recent Dictations</h2>

      {loading ? (
        <div className="flex justify-center py-8">
          <Loader2 className="w-6 h-6 text-zinc-500 animate-spin" />
        </div>
      ) : history.length === 0 ? (
        <div className="text-center py-12 text-zinc-500 text-sm">
          No dictations yet.
        </div>
      ) : (
        <div className="space-y-3">
          {history.map((item) => (
            <div key={item.id} className="bg-zinc-800/40 border border-zinc-800 rounded-xl p-4 group relative">
              <p className="text-zinc-200 text-sm mb-2 pr-8">{item.text}</p>
              <div className="flex items-center justify-between text-[10px] text-zinc-500 font-mono">
                <span>{item.timestamp?.toDate().toLocaleString() || 'Just now'}</span>
                {item.durationMs && <span>{(item.durationMs / 1000).toFixed(1)}s</span>}
              </div>

              <button
                onClick={async () => {
                  try {
                    await navigator.clipboard.writeText(item.text);
                    toast.success("Copied!");
                  } catch (err) {
                    console.warn("Clipboard write failed:", err);
                    toast.error("Failed to copy to clipboard.");
                  }
                }}
                className="absolute top-3 right-3 p-1.5 bg-zinc-700/0 text-zinc-400 rounded-md opacity-0 group-hover:opacity-100 transition-all hover:bg-zinc-700 hover:text-white"
              >
                <Copy size={14} />
              </button>
            </div>
          ))}
        </div>
      )}
    </motion.div>
  );
}

function SettingsView({ user }: { user: User; key?: string }) {
  const [shortcut, setShortcut] = useState<string[]>(['Space']);
  const [language, setLanguage] = useState<string>('Auto-detect');
  const [autoCopy, setAutoCopy] = useState<boolean>(true);
  const [autoClear, setAutoClear] = useState<boolean>(false);
  const [microphoneId, setMicrophoneId] = useState<string>('default');
  const [apiKey, setApiKey] = useState<string>('');
  const [systemPrompt, setSystemPrompt] = useState<string>('Transcribe the audio exactly as spoken.');
  const [audioDevices, setAudioDevices] = useState<MediaDeviceInfo[]>([]);

  const [isRecordingShortcut, setIsRecordingShortcut] = useState(false);
  const [tempShortcut, setTempShortcut] = useState<Set<string>>(new Set());

  useEffect(() => {
    const loadSettings = async () => {
      const path = `users/${user.uid}`;
      try {
        const docSnap = await getDoc(doc(db, path));
        if (docSnap.exists()) {
          const data = docSnap.data();
          if (data.shortcut) {
            const rawShortcut = data.shortcut;
            setShortcut(Array.isArray(rawShortcut) ? rawShortcut : [rawShortcut]);
          }
          if (data.language) setLanguage(data.language);
          if (data.autoCopy !== undefined) setAutoCopy(data.autoCopy);
          if (data.autoClear !== undefined) setAutoClear(data.autoClear);
          if (data.microphoneId) setMicrophoneId(data.microphoneId);
          if (data.apiKey) setApiKey(data.apiKey);
          if (data.systemPrompt) setSystemPrompt(data.systemPrompt);
        }
      } catch (e) {
        handleFirestoreError(e, OperationType.GET, path);
      }
    };
    loadSettings();
  }, [user.uid]);

  const updateSetting = async (key: string, value: any) => {
    const path = `users/${user.uid}`;
    try {
      await setDoc(doc(db, path), { [key]: value }, { merge: true });
      toast.success("Settings updated");
    } catch (err) {
      handleFirestoreError(err, OperationType.WRITE, path);
    }
  };

  useEffect(() => {
    const getDevices = async () => {
      try {
        // Request permission briefly to ensure device labels are exposed
        await navigator.mediaDevices.getUserMedia({ audio: true }).then(s => s.getTracks().forEach(t => t.stop())).catch(() => { });
        const devices = await navigator.mediaDevices.enumerateDevices();
        setAudioDevices(devices.filter(d => d.kind === 'audioinput'));
      } catch (err) {
        console.error("Failed to enumerate audio devices", err);
      }
    };
    getDevices();
  }, []);

  // Sync settings to Python backend if running in pywebview
  useEffect(() => {
    if ((window as any).pywebview && (window as any).pywebview.api) {
      setTimeout(() => {
        try {
          (window as any).pywebview.api.sync_settings(JSON.stringify({
            shortcut, apiKey, systemPrompt, language, autoCopy, autoClear
          }));
        } catch (e) {
          console.error("Fast sync failed:", e);
        }
      }, 100);
    }
  }, [shortcut, apiKey, systemPrompt, language]);

  useEffect(() => {
    if (!isRecordingShortcut) return;

    const handleKeyDown = (e: KeyboardEvent) => {
      e.preventDefault();
      setTempShortcut(prev => new Set(prev).add(e.code));
    };

    const handleKeyUp = async (e: KeyboardEvent) => {
      e.preventDefault();
      if (tempShortcut.size > 0) {
        const newShortcut = Array.from(tempShortcut);
        setShortcut(newShortcut);
        setIsRecordingShortcut(false);
        setTempShortcut(new Set());

        const path = `users/${user.uid}`;
        try {
          await setDoc(doc(db, path), { shortcut: newShortcut }, { merge: true });
          toast.success("Shortcut updated");
        } catch (err) {
          handleFirestoreError(err, OperationType.WRITE, path);
        }
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    window.addEventListener('keyup', handleKeyUp);
    return () => {
      window.removeEventListener('keydown', handleKeyDown);
      window.removeEventListener('keyup', handleKeyUp);
    };
  }, [isRecordingShortcut, tempShortcut, user.uid]);

  const formatKey = (code: string) => {
    return code
      .replace('Key', '')
      .replace('Digit', '')
      .replace('Left', '')
      .replace('Right', '')
      .replace('Space', '␣ Space');
  };

  return (
    <motion.div
      initial={{ opacity: 0, x: 20 }}
      animate={{ opacity: 1, x: 0 }}
      exit={{ opacity: 0, x: -20 }}
      className="absolute inset-0 p-6"
    >
      <div className="flex items-center gap-4 mb-8">
        <img src={user.photoURL || ''} alt="Profile" className="w-12 h-12 rounded-full border border-zinc-700" />
        <div>
          <div className="font-medium text-zinc-100">{user.displayName}</div>
          <div className="text-xs text-zinc-500">{user.email}</div>
        </div>
      </div>

      <div className="space-y-6 pb-20">
        <div className="bg-zinc-800/50 border border-zinc-700/50 rounded-2xl p-5">
          <div className="flex items-center justify-between mb-4">
            <div>
              <h3 className="text-sm font-medium text-zinc-100">Dictation Shortcut</h3>
              <p className="text-xs text-zinc-500 mt-1">Hold these keys to record</p>
            </div>
            <Settings className="w-4 h-4 text-zinc-600" />
          </div>

          <div className="flex flex-wrap gap-2 mb-4">
            {(isRecordingShortcut ? Array.from(tempShortcut) : (Array.isArray(shortcut) ? shortcut : [])).map((key) => (
              <span key={key} className="px-3 py-1.5 bg-zinc-700 text-zinc-100 rounded-lg text-xs font-mono border border-zinc-600 shadow-sm">
                {formatKey(key)}
              </span>
            ))}
            {(isRecordingShortcut && tempShortcut.size === 0) && (
              <span className="text-xs text-zinc-400 animate-pulse">Press keys...</span>
            )}
          </div>

          <button
            onClick={() => {
              setIsRecordingShortcut(true);
              setTempShortcut(new Set());
            }}
            className={`w-full py-2.5 rounded-xl text-sm font-medium transition-all ${isRecordingShortcut
              ? 'bg-blue-500 text-white shadow-[0_0_20px_rgba(59,130,246,0.3)]'
              : 'bg-zinc-700 text-zinc-200 hover:bg-zinc-600'
              }`}
          >
            {isRecordingShortcut ? 'Recording Shortcut...' : 'Change Shortcut'}
          </button>
        </div>

        <div className="bg-zinc-800/50 border border-zinc-700/50 rounded-2xl p-5 space-y-4">
          <div className="flex items-center justify-between">
            <div>
              <h3 className="text-sm font-medium text-zinc-100">Microphone</h3>
              <p className="text-xs text-zinc-500 mt-1">Audio input source</p>
            </div>
            <select
              value={microphoneId}
              onChange={(e) => {
                setMicrophoneId(e.target.value);
                updateSetting('microphoneId', e.target.value);
              }}
              className="bg-zinc-700 border border-zinc-600 text-zinc-100 text-sm rounded-lg focus:ring-blue-500 focus:border-blue-500 block p-2 max-w-[150px] truncate"
            >
              <option value="default">System Default</option>
              {audioDevices.map(device => (
                <option key={device.deviceId} value={device.deviceId}>
                  {device.label || `Microphone (${device.deviceId.slice(0, 4)}...)`}
                </option>
              ))}
            </select>
          </div>

          <div className="h-px bg-zinc-700/50 w-full" />

          <div className="space-y-3">
            <div>
              <h3 className="text-sm font-medium text-zinc-100">API Key</h3>
              <p className="text-xs text-zinc-500 mt-1 mb-2">Gemini API Key required for dictation</p>
              <div className="flex gap-2">
                <input
                  type="password"
                  value={apiKey}
                  onChange={(e) => setApiKey(e.target.value)}
                  placeholder="AIzaSy..."
                  className="flex-1 bg-zinc-700 border border-zinc-600 text-zinc-100 text-sm rounded-lg focus:ring-blue-500 focus:border-blue-500 block p-2.5"
                />
                <button
                  onClick={() => {
                    localStorage.setItem('dictando_apiKey', apiKey);
                    updateSetting('apiKey', apiKey);
                  }}
                  className="px-4 bg-blue-600 hover:bg-blue-500 text-white text-sm font-medium rounded-lg transition-colors shrink-0"
                >
                  Save
                </button>
              </div>
            </div>

            <div>
              <h3 className="text-sm font-medium text-zinc-100">System Prompt</h3>
              <p className="text-xs text-zinc-500 mt-1 mb-2">Formatting instructions for the transcriber</p>
              <textarea
                value={systemPrompt}
                onChange={(e) => setSystemPrompt(e.target.value)}
                placeholder="Transcribe exactly as spoken..."
                className="w-full bg-zinc-700 border border-zinc-600 text-zinc-100 text-sm rounded-lg focus:ring-blue-500 focus:border-blue-500 block p-2.5 min-h-[80px]"
              />
              <button
                onClick={() => updateSetting('systemPrompt', systemPrompt)}
                className="mt-2 px-4 py-2 bg-blue-600 hover:bg-blue-500 text-white text-sm font-medium rounded-lg transition-colors"
              >
                Save Prompt
              </button>
            </div>
          </div>

          <div className="h-px bg-zinc-700/50 w-full" />

          <div className="flex items-center justify-between">
            <div>
              <h3 className="text-sm font-medium text-zinc-100">Language</h3>
              <p className="text-xs text-zinc-500 mt-1">Transcription language</p>
            </div>
            <select
              value={language}
              onChange={(e) => {
                setLanguage(e.target.value);
                updateSetting('language', e.target.value);
              }}
              className="bg-zinc-700 border border-zinc-600 text-zinc-100 text-sm rounded-lg focus:ring-blue-500 focus:border-blue-500 block p-2"
            >
              <option value="Auto-detect">Auto-detect</option>
              <option value="English">English</option>
              <option value="Spanish">Spanish</option>
              <option value="French">French</option>
              <option value="German">German</option>
              <option value="Italian">Italian</option>
              <option value="Portuguese">Portuguese</option>
              <option value="Romanian">Romanian</option>
              <option value="Japanese">Japanese</option>
              <option value="Korean">Korean</option>
              <option value="Chinese">Chinese</option>
            </select>
          </div>

          <div className="h-px bg-zinc-700/50 w-full" />

          <div className="flex items-center justify-between">
            <div>
              <h3 className="text-sm font-medium text-zinc-100">Auto-copy</h3>
              <p className="text-xs text-zinc-500 mt-1">Copy text to clipboard</p>
            </div>
            <label className="relative inline-flex items-center cursor-pointer">
              <input
                type="checkbox"
                className="sr-only peer"
                checked={autoCopy}
                onChange={(e) => {
                  setAutoCopy(e.target.checked);
                  updateSetting('autoCopy', e.target.checked);
                }}
              />
              <div className="w-11 h-6 bg-zinc-700 peer-focus:outline-none rounded-full peer peer-checked:after:translate-x-full peer-checked:after:border-white after:content-[''] after:absolute after:top-[2px] after:left-[2px] after:bg-white after:border-gray-300 after:border after:rounded-full after:h-5 after:w-5 after:transition-all peer-checked:bg-blue-500"></div>
            </label>
          </div>

          <div className="h-px bg-zinc-700/50 w-full" />

          <div className="flex items-center justify-between">
            <div>
              <h3 className="text-sm font-medium text-zinc-100">Auto-clear</h3>
              <p className="text-xs text-zinc-500 mt-1">Clear text after 3 seconds</p>
            </div>
            <label className="relative inline-flex items-center cursor-pointer">
              <input
                type="checkbox"
                className="sr-only peer"
                checked={autoClear}
                onChange={(e) => {
                  setAutoClear(e.target.checked);
                  updateSetting('autoClear', e.target.checked);
                }}
              />
              <div className="w-11 h-6 bg-zinc-700 peer-focus:outline-none rounded-full peer peer-checked:after:translate-x-full peer-checked:after:border-white after:content-[''] after:absolute after:top-[2px] after:left-[2px] after:bg-white after:border-gray-300 after:border after:rounded-full after:h-5 after:w-5 after:transition-all peer-checked:bg-blue-500"></div>
            </label>
          </div>
        </div>

        <div className="bg-amber-500/10 border border-amber-500/20 rounded-xl p-4">
          <p className="text-[11px] text-amber-200/80 leading-relaxed">
            <span className="font-bold block mb-1 text-amber-400">Note on Global Shortcuts:</span>
            As a web app, shortcuts only work when this tab is active. For true system-wide shortcuts on macOS, export the project and build it as a native app using Tauri or Electron.
          </p>
        </div>

        <button
          onClick={() => signOut(auth)}
          className="w-full py-3 border border-zinc-800 text-zinc-500 hover:text-red-400 hover:border-red-400/30 rounded-xl text-sm font-medium transition-all flex items-center justify-center gap-2"
        >
          <LogOut size={16} /> Sign Out
        </button>
      </div>

    </motion.div>
  );
}
