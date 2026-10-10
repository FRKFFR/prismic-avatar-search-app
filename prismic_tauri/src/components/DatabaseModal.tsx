import React from 'react';
import {
  X,
  Database,
  RefreshCw,
  HardDrive,
  Users,
  Calendar,
  Layers,
  CheckCircle,
  AlertCircle,
} from 'lucide-react';
import type { DatabaseStatus, SyncProgress } from '../types';

interface DatabaseModalProps {
  isOpen: boolean;
  onClose: () => void;
  status: DatabaseStatus | null;
  progress: SyncProgress | null;
  isSyncing: boolean;
  onStartSync: () => void;
}

export const DatabaseModal: React.FC<DatabaseModalProps> = ({
  isOpen,
  onClose,
  status,
  progress,
  isSyncing,
  onStartSync,
}) => {
  if (!isOpen) return null;

  const fileSizeMb = status
    ? (status.file_size_bytes / (1024 * 1024)).toFixed(1)
    : '0';

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/80 backdrop-blur-sm animate-in fade-in duration-200">
      <div
        className="relative w-full max-w-lg bg-[#0f1422] border border-gray-800 rounded-3xl overflow-hidden shadow-2xl flex flex-col"
        onClick={(e) => e.stopPropagation()}
      >
        {/* Header */}
        <div className="flex items-center justify-between px-6 py-4 border-b border-gray-800 bg-gray-900/60">
          <div className="flex items-center gap-2.5">
            <div className="p-2 rounded-xl bg-cyan-950/60 border border-cyan-800/40 text-cyan-400">
              <Database className="w-4 h-4" />
            </div>
            <div>
              <h2 className="text-sm font-bold text-gray-100">Database Management</h2>
              <span className="text-[11px] text-gray-400">
                Local avatar SQLite database & synchronization
              </span>
            </div>
          </div>
          <button
            onClick={onClose}
            className="p-1.5 rounded-xl hover:bg-gray-800 text-gray-400 hover:text-white transition-colors"
          >
            <X className="w-5 h-5" />
          </button>
        </div>

        {/* Content */}
        <div className="p-6 space-y-5">
          {/* Status Metrics */}
          <div className="grid grid-cols-2 gap-3">
            <div className="p-3.5 rounded-xl bg-gray-900/70 border border-gray-800 flex items-center gap-3">
              <div className="p-2 rounded-lg bg-cyan-950/50 text-cyan-400">
                <Layers className="w-4 h-4" />
              </div>
              <div>
                <span className="text-[10px] text-gray-400 uppercase font-semibold block">
                  Total Avatars
                </span>
                <span className="text-sm font-bold text-gray-100">
                  {status ? status.avatar_count.toLocaleString() : '0'}
                </span>
              </div>
            </div>

            <div className="p-3.5 rounded-xl bg-gray-900/70 border border-gray-800 flex items-center gap-3">
              <div className="p-2 rounded-lg bg-teal-950/50 text-teal-400">
                <Users className="w-4 h-4" />
              </div>
              <div>
                <span className="text-[10px] text-gray-400 uppercase font-semibold block">
                  Distinct Authors
                </span>
                <span className="text-sm font-bold text-gray-100">
                  {status ? status.author_count.toLocaleString() : '0'}
                </span>
              </div>
            </div>

            <div className="p-3.5 rounded-xl bg-gray-900/70 border border-gray-800 flex items-center gap-3">
              <div className="p-2 rounded-lg bg-purple-950/50 text-purple-400">
                <Calendar className="w-4 h-4" />
              </div>
              <div>
                <span className="text-[10px] text-gray-400 uppercase font-semibold block">
                  Database Date
                </span>
                <span className="text-sm font-bold text-gray-100 flex items-center gap-1.5">
                  {status?.last_updated || 'Never'}
                  {status?.is_stale && (
                    <span className="px-1.5 py-0.5 rounded text-[9px] font-bold bg-amber-500/15 text-amber-400 border border-amber-500/30">
                      OUTDATED
                    </span>
                  )}
                </span>
              </div>
            </div>

            <div className="p-3.5 rounded-xl bg-gray-900/70 border border-gray-800 flex items-center gap-3">
              <div className="p-2 rounded-lg bg-indigo-950/50 text-indigo-400">
                <HardDrive className="w-4 h-4" />
              </div>
              <div>
                <span className="text-[10px] text-gray-400 uppercase font-semibold block">
                  File Size
                </span>
                <span className="text-sm font-bold text-gray-100">{fileSizeMb} MB</span>
              </div>
            </div>
          </div>

          {/* Sync Progress Indicator */}
          {isSyncing && progress && (
            <div className="p-4 rounded-xl bg-cyan-950/20 border border-cyan-800/40 space-y-2.5 animate-pulse">
              <div className="flex items-center justify-between text-xs">
                <span className="font-semibold text-cyan-300">
                  Step {progress.step} of {progress.total_steps}: {progress.stage}
                </span>
                <span className="font-mono text-cyan-400 font-bold">
                  {progress.percent.toFixed(0)}%
                </span>
              </div>

              {/* Progress Track */}
              <div className="h-2 w-full bg-gray-900 rounded-full overflow-hidden">
                <div
                  className="h-full bg-gradient-to-r from-cyan-500 to-teal-400 transition-all duration-300 rounded-full"
                  style={{ width: `${Math.min(100, Math.max(5, progress.percent))}%` }}
                />
              </div>

              <p className="text-[11px] text-gray-400 truncate">{progress.message}</p>
            </div>
          )}

          {progress?.error && (
            <div className="p-3.5 rounded-xl bg-rose-950/40 border border-rose-800/50 flex items-start gap-2.5 text-xs text-rose-300">
              <AlertCircle className="w-4 h-4 text-rose-400 shrink-0 mt-0.5" />
              <span>{progress.error}</span>
            </div>
          )}

          {progress?.complete && !isSyncing && (
            <div className="p-3.5 rounded-xl bg-emerald-950/40 border border-emerald-800/50 flex items-center gap-2.5 text-xs text-emerald-300">
              <CheckCircle className="w-4 h-4 text-emerald-400 shrink-0" />
              <span>Database synchronization completed successfully!</span>
            </div>
          )}

          {/* Sync Action Button */}
          <button
            onClick={onStartSync}
            disabled={isSyncing}
            className="w-full flex items-center justify-center gap-2 py-3 px-4 rounded-xl bg-gradient-to-r from-cyan-600 to-teal-600 hover:from-cyan-500 hover:to-teal-500 text-white text-xs font-bold shadow-lg shadow-cyan-600/30 transition-all disabled:opacity-50"
          >
            <RefreshCw className={`w-4 h-4 ${isSyncing ? 'animate-spin' : ''}`} />
            <span>{isSyncing ? 'Synchronizing...' : 'Download & Re-sync Database'}</span>
          </button>

          <p className="text-[11px] text-gray-500 text-center leading-relaxed">
            Database files are hosted on GitHub Gist and automatically updated daily by Prismic.
            Re-syncing downloads the PC, Quest, and iOS indexes and builds a fast SQLite search index.
          </p>
        </div>
      </div>
    </div>
  );
};
