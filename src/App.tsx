import React, { useState, useEffect, useRef } from 'react';
import { auth, db, googleProvider } from './firebase';
import { signInWithPopup, signOut, onAuthStateChanged, User } from 'firebase/auth';
import { collection, doc, setDoc, getDoc, addDoc, query, orderBy, limit, onSnapshot, serverTimestamp } from 'firebase/firestore';
import { GoogleGenAI } from '@google/genai';
import { Mic, Settings, History, LogOut, Play, Square, Loader2, Copy, Check } from 'lucide-react';
import { Toaster, toast } from 'sonner';
import { motion, AnimatePresence } from 'motion/react';

// Initialize Gemini API
const ai = new GoogleGenAI({ apiKey: (import.meta as any).env.VITE_GEMINI_API_KEY || process.env.GEMINI_API_KEY });


export default function App() {
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
        const userSnap = await getDoc(userRef);
        if (!userSnap.exists()) {
          await setDoc(userRef, {
            email: currentUser.email,
            displayName: currentUser.displayName || '',
            photoURL: currentUser.photoURL || '',
            shortcut: 'Option',
            createdAt: serverTimestamp()
          });
        }
      }
    });
    return () => unsubscribe();
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
      <Toaster theme="dark" position="bottom-center" />
      
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
      className={`flex flex-col items-center justify-center w-16 h-12 rounded-xl transition-all ${
        isActive ? 'text-blue-400 bg-blue-500/10' : 'text-zinc-500 hover:text-zinc-300 hover:bg-zinc-800/50'
      }`}
    >
      {icon}
      <span className="text-[10px] font-medium mt-1">{label}</span>
    </button>
  );
}

function LoginScreen() {
  const handleLogin = async () => {
    try {
      await signInWithPopup(auth, googleProvider);
    } catch (error) {
      console.error("Login failed", error);
      toast.error("Failed to sign in");
    }
  };

  return (
    <div className="min-h-screen bg-zinc-950 flex flex-col items-center justify-center p-4">
      <div className="w-full max-w-sm bg-zinc-900 border border-zinc-800 rounded-3xl p-8 text-center shadow-2xl">
        <div className="w-16 h-16 bg-blue-500/20 text-blue-400 rounded-2xl flex items-center justify-center mx-auto mb-6">
          <Mic size={32} />
        </div>
        <h1 className="text-2xl font-semibold text-zinc-100 mb-2">Dictando</h1>
        <p className="text-zinc-400 text-sm mb-8">Instant, multilingual speech-to-text for your Mac.</p>
        
        <button
          onClick={handleLogin}
          className="w-full bg-zinc-100 text-zinc-900 hover:bg-white font-medium py-3 px-4 rounded-xl transition-colors flex items-center justify-center gap-2"
        >
          <svg className="w-5 h-5" viewBox="0 0 24 24">
            <path fill="currentColor" d="M22.56 12.25c0-.78-.07-1.53-.2-2.25H12v4.26h5.92c-.26 1.37-1.04 2.53-2.21 3.31v2.77h3.57c2.08-1.92 3.28-4.74 3.28-8.09z" />
            <path fill="#34A853" d="M12 23c2.97 0 5.46-.98 7.28-2.66l-3.57-2.77c-.98.66-2.23 1.06-3.71 1.06-2.86 0-5.29-1.93-6.16-4.53H2.18v2.84C3.99 20.53 7.7 23 12 23z" />
            <path fill="#FBBC05" d="M5.84 14.09c-.22-.66-.35-1.36-.35-2.09s.13-1.43.35-2.09V7.07H2.18C1.43 8.55 1 10.22 1 12s.43 3.45 1.18 4.93l2.85-2.22.81-.62z" />
            <path fill="#EA4335" d="M12 5.38c1.62 0 3.06.56 4.21 1.64l3.15-3.15C17.45 2.09 14.97 1 12 1 7.7 1 3.99 3.47 2.18 7.07l3.66 2.84c.87-2.6 3.3-4.53 6.16-4.53z" />
          </svg>
          Continue with Google
        </button>
      </div>
    </div>
  );
}

function RecordView({ user }: { user: User; key?: string }) {
  const [isRecording, setIsRecording] = useState(false);
  const [isProcessing, setIsProcessing] = useState(false);
  const [transcription, setTranscription] = useState<string | null>(null);
  const mediaRecorderRef = useRef<MediaRecorder | null>(null);
  const audioChunksRef = useRef<Blob[]>([]);
  const startTimeRef = useRef<number>(0);

  // Simulate global shortcut with spacebar when this view is focused
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.code === 'Space' && !e.repeat && !isRecording && !isProcessing) {
        e.preventDefault();
        startRecording();
      }
    };
    const handleKeyUp = (e: KeyboardEvent) => {
      if (e.code === 'Space' && isRecording) {
        e.preventDefault();
        stopRecording();
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    window.addEventListener('keyup', handleKeyUp);
    return () => {
      window.removeEventListener('keydown', handleKeyDown);
      window.removeEventListener('keyup', handleKeyUp);
    };
  }, [isRecording, isProcessing]);

  const startRecording = async () => {
    try {
      const stream = await navigator.mediaDevices.getUserMedia({ audio: true });
      const mediaRecorder = new MediaRecorder(stream);
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
        const audioBlob = new Blob(audioChunksRef.current, { type: 'audio/webm' });
        
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
          const response = await ai.models.generateContent({
            model: 'gemini-3.1-flash-preview',
            contents: {
              parts: [
                {
                  inlineData: {
                    mimeType: audioBlob.type || 'audio/webm',
                    data: base64String
                  }
                },
                {
                  text: "Transcribe the following audio exactly as spoken, in its original language. Do not add any extra text, commentary, or markdown formatting. Just the raw text."
                }
              ]
            }
          });

          const text = response.text?.trim() || "";
          setTranscription(text);
          
          if (text) {
            // Save to Firestore
            await addDoc(collection(db, 'users', user.uid, 'dictations'), {
              text,
              timestamp: serverTimestamp(),
              durationMs
            });
            
            // Auto-copy to clipboard (simulating the "paste into active window" feature of a native app)
            navigator.clipboard.writeText(text);
            toast.success("Copied to clipboard!");
          }
        } catch (apiError) {
          console.error("Gemini API Error:", apiError);
          toast.error("Transcription failed");
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
            <span className="text-zinc-500 text-sm">Hold Spacebar to dictate</span>
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
          className={`relative w-32 h-32 rounded-full flex items-center justify-center transition-all duration-300 ${
            isRecording 
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
                onClick={() => {
                  navigator.clipboard.writeText(transcription);
                  toast.success("Copied!");
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
    const q = query(
      collection(db, 'users', user.uid, 'dictations'),
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
      console.error("History error:", error);
      toast.error("Failed to load history");
      setLoading(false);
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
                onClick={() => {
                  navigator.clipboard.writeText(item.text);
                  toast.success("Copied!");
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
  const [shortcut, setShortcut] = useState('Option');
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    const loadSettings = async () => {
      try {
        const docSnap = await getDoc(doc(db, 'users', user.uid));
        if (docSnap.exists() && docSnap.data().shortcut) {
          setShortcut(docSnap.data().shortcut);
        }
      } catch (e) {
        console.error(e);
      }
    };
    loadSettings();
  }, [user.uid]);

  const saveShortcut = async (newShortcut: string) => {
    setShortcut(newShortcut);
    setSaving(true);
    try {
      await setDoc(doc(db, 'users', user.uid), { shortcut: newShortcut }, { merge: true });
      toast.success("Settings saved");
    } catch (e) {
      toast.error("Failed to save settings");
    } finally {
      setSaving(false);
    }
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

      <div className="space-y-6">
        <div>
          <label className="block text-xs font-medium text-zinc-400 uppercase tracking-wider mb-3">
            Global Shortcut
          </label>
          <div className="grid grid-cols-2 gap-2">
            {['Option', 'Command', 'Control', 'Shift'].map(key => (
              <button
                key={key}
                onClick={() => saveShortcut(key)}
                className={`py-2 px-3 rounded-lg text-sm font-medium transition-colors flex items-center justify-between ${
                  shortcut === key 
                    ? 'bg-blue-500/20 text-blue-400 border border-blue-500/30' 
                    : 'bg-zinc-800/50 text-zinc-300 border border-zinc-800 hover:bg-zinc-800'
                }`}
              >
                {key}
                {shortcut === key && <Check size={14} />}
              </button>
            ))}
          </div>
          <p className="text-[10px] text-zinc-500 mt-3 leading-relaxed">
            In this web prototype, use the <strong className="text-zinc-300">Spacebar</strong> to dictate while the app is focused. When wrapped in Tauri/Electron, this setting will bind to the global OS shortcut.
          </p>
        </div>

        <div className="pt-6 border-t border-zinc-800/50">
          <button
            onClick={() => signOut(auth)}
            className="w-full py-3 px-4 rounded-xl bg-red-500/10 text-red-400 hover:bg-red-500/20 font-medium text-sm transition-colors flex items-center justify-center gap-2"
          >
            <LogOut size={16} />
            Sign Out
          </button>
        </div>
      </div>
    </motion.div>
  );
}
