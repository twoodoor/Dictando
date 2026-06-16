import React, { useState, useEffect } from 'react';
import { db } from '../firebase';
import { collection, query, orderBy, limit, onSnapshot, deleteDoc, doc } from 'firebase/firestore';
import { User } from 'firebase/auth';
import { Copy, Trash2, Loader2, Search } from 'lucide-react';
import { toast } from 'sonner';
import { motion } from 'motion/react';
import { isNative, backend, events } from '../lib/bridge';

export function HistoryView({ user }: { user: User | null }) {
  const [history, setHistory] = useState<any[]>([]);
  const [loading, setLoading] = useState(true);
  const [search, setSearch] = useState('');
  const [deleting, setDeleting] = useState<string | null>(null);

  // Native: local history (SQLite) + live append from transcription events.
  useEffect(() => {
    if (!isNative) return;
    backend.listHistory(50).then((items) => { setHistory(items); setLoading(false); }).catch(() => setLoading(false));
    const off = events.onTranscription((r) => {
      if (!r.text) return;
      setHistory((prev) => [{ ...r, favorite: false }, ...prev]);
    });
    return off;
  }, []);

  // Web: Firestore-backed history (requires a logged-in user).
  useEffect(() => {
    if (isNative) return;
    if (!user) { setLoading(false); return; }
    const q = query(collection(db, `users/${user.uid}/dictations`), orderBy('timestamp', 'desc'), limit(50));
    const unsub = onSnapshot(q, (snap) => {
      setHistory(snap.docs.map((d) => ({ id: d.id, ...d.data() })));
      setLoading(false);
    }, () => setLoading(false));
    return unsub;
  }, [user]);

  const handleDelete = async (id: string) => {
    setDeleting(id);
    try {
      if (isNative) { await backend.deleteHistory(id); setHistory((prev) => prev.filter((h) => h.id !== id)); }
      else if (user) { await deleteDoc(doc(db, `users/${user.uid}/dictations`, id)); }
      toast.success('Deleted');
    } catch { toast.error('Delete failed'); }
    setDeleting(null);
  };

  const filtered = history.filter((item) => !search || item.text?.toLowerCase().includes(search.toLowerCase()));

  const when = (item: any) =>
    typeof item.timestamp === 'number'
      ? new Date(item.timestamp).toLocaleString()
      : (item.timestamp?.toDate?.().toLocaleString() ?? 'Just now');

  return (
    <motion.div
      initial={{ opacity: 0, y: 8 }} animate={{ opacity: 1, y: 0 }} exit={{ opacity: 0, y: -8 }}
      className="absolute inset-0 flex flex-col"
    >
      <div className="max-w-3xl w-full mx-auto px-8 pt-8 flex-1 flex flex-col min-h-0">
        <h1 className="font-display text-3xl text-fg">History</h1>
        <div className="relative mt-4 mb-3">
          <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-faint pointer-events-none" />
          <input
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            placeholder="Search transcriptions…"
            className="w-full bg-surface border border-line text-fg text-sm rounded-xl pl-9 pr-4 py-2.5 placeholder:text-faint focus:outline-none focus:border-accent"
          />
        </div>

        <div className="flex-1 overflow-y-auto pb-6 space-y-2">
          {loading ? (
            <div className="flex justify-center py-12"><Loader2 className="w-6 h-6 text-muted animate-spin" /></div>
          ) : filtered.length === 0 ? (
            <div className="text-center py-16 text-muted text-sm">{search ? 'No results' : 'No transcriptions yet'}</div>
          ) : (
            filtered.map((item) => (
              <div key={item.id} className="bg-surface border border-line rounded-2xl p-4 group relative">
                <p className="text-fg text-sm leading-relaxed pr-16">{item.text}</p>
                <div className="flex items-center gap-2 mt-2 text-[10px] font-mono text-faint">
                  <span>{when(item)}</span>
                  {!!item.durationMs && <span>· {(item.durationMs / 1000).toFixed(1)}s</span>}
                  {item.engine && <span>· {item.engine}</span>}
                </div>
                <div className="absolute top-3 right-3 flex gap-1 opacity-0 group-hover:opacity-100 transition-opacity">
                  <button
                    onClick={() => navigator.clipboard.writeText(item.text).then(() => toast.success('Copied!'))}
                    className="p-1.5 bg-surface-2 text-muted rounded-lg hover:text-fg transition-colors"
                  ><Copy size={12} /></button>
                  <button
                    onClick={() => handleDelete(item.id)}
                    disabled={deleting === item.id}
                    className="p-1.5 bg-surface-2 text-muted rounded-lg hover:text-danger transition-colors"
                  >{deleting === item.id ? <Loader2 size={12} className="animate-spin" /> : <Trash2 size={12} />}</button>
                </div>
              </div>
            ))
          )}
        </div>
      </div>
    </motion.div>
  );
}
