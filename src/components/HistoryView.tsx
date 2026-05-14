import React, { useState, useEffect } from 'react';
import { db } from '../firebase';
import { collection, query, orderBy, limit, onSnapshot, deleteDoc, doc } from 'firebase/firestore';
import { User } from 'firebase/auth';
import { Copy, Trash2, Loader2, Search } from 'lucide-react';
import { toast } from 'sonner';
import { motion } from 'motion/react';

const ENGINE_COLOURS: Record<string, string> = {
  'faster-whisper': 'text-emerald-400',
  'google':         'text-blue-400',
  'gemini':         'text-purple-400',
};

export function HistoryView({ user }: { user: User }) {
  const [history, setHistory] = useState<any[]>([]);
  const [loading, setLoading] = useState(true);
  const [search, setSearch] = useState('');
  const [deleting, setDeleting] = useState<string | null>(null);

  useEffect(() => {
    const q = query(
      collection(db, `users/${user.uid}/dictations`),
      orderBy('timestamp', 'desc'),
      limit(50),
    );
    const unsub = onSnapshot(q, (snap) => {
      setHistory(snap.docs.map(d => ({ id: d.id, ...d.data() })));
      setLoading(false);
    }, () => setLoading(false));
    return unsub;
  }, [user.uid]);

  const handleDelete = async (id: string) => {
    setDeleting(id);
    try {
      await deleteDoc(doc(db, `users/${user.uid}/dictations`, id));
      toast.success('Deleted');
    } catch { toast.error('Delete failed'); }
    setDeleting(null);
  };

  const filtered = history.filter(item =>
    !search || item.text?.toLowerCase().includes(search.toLowerCase())
  );

  return (
    <motion.div
      initial={{ opacity: 0, x: 16 }} animate={{ opacity: 1, x: 0 }} exit={{ opacity: 0, x: -16 }}
      className="absolute inset-0 flex flex-col"
    >
      {/* Search bar */}
      <div className="px-4 pt-4 pb-2 shrink-0">
        <div className="relative">
          <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-zinc-500 pointer-events-none" />
          <input
            value={search}
            onChange={e => setSearch(e.target.value)}
            placeholder="Search dictations..."
            className="w-full bg-zinc-800/60 border border-zinc-700/50 text-zinc-200 text-sm rounded-xl pl-9 pr-4 py-2.5 placeholder:text-zinc-600 focus:outline-none focus:border-blue-500/50 focus:ring-1 focus:ring-blue-500/20"
          />
        </div>
      </div>

      {/* List */}
      <div className="flex-1 overflow-y-auto px-4 pb-4 space-y-2">
        {loading ? (
          <div className="flex justify-center py-12">
            <Loader2 className="w-6 h-6 text-zinc-500 animate-spin" />
          </div>
        ) : filtered.length === 0 ? (
          <div className="text-center py-16 text-zinc-600 text-sm">
            {search ? 'No results' : 'No dictations yet'}
          </div>
        ) : (
          filtered.map((item) => (
            <div key={item.id} className="bg-zinc-800/40 border border-zinc-800 rounded-xl p-3.5 group relative">
              <p className="text-zinc-200 text-sm leading-relaxed pr-14">{item.text}</p>
              <div className="flex items-center gap-2 mt-2">
                <span className="text-[10px] text-zinc-600 font-mono">
                  {item.timestamp?.toDate?.().toLocaleString() ?? 'Just now'}
                </span>
                {item.durationMs && (
                  <span className="text-[10px] text-zinc-600 font-mono">
                    · {(item.durationMs / 1000).toFixed(1)}s
                  </span>
                )}
                {item.engine && (
                  <span className={`text-[10px] font-mono ${ENGINE_COLOURS[item.engine] || 'text-zinc-500'}`}>
                    · {item.engine}
                  </span>
                )}
              </div>

              {/* Action buttons */}
              <div className="absolute top-3 right-3 flex gap-1 opacity-0 group-hover:opacity-100 transition-opacity">
                <button
                  onClick={() => navigator.clipboard.writeText(item.text).then(() => toast.success('Copied!'))}
                  className="p-1.5 bg-zinc-700 text-zinc-400 rounded-lg hover:text-white transition-colors"
                >
                  <Copy size={12} />
                </button>
                <button
                  onClick={() => handleDelete(item.id)}
                  disabled={deleting === item.id}
                  className="p-1.5 bg-zinc-700 text-zinc-400 rounded-lg hover:text-red-400 transition-colors"
                >
                  {deleting === item.id
                    ? <Loader2 size={12} className="animate-spin" />
                    : <Trash2 size={12} />
                  }
                </button>
              </div>
            </div>
          ))
        )}
      </div>
    </motion.div>
  );
}
